use serde::{Serialize, Deserialize};
use std::time::SystemTime;
use std::collections::HashMap;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{SyncClient, error::SyncError, DagNode, DagNodeMetadata};
use wallet_types::WalletResult;

/// Trust bundle containing verified DIDs and credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    /// Trust bundle ID
    pub id: String,
    
    /// Trust bundle name
    pub name: String,
    
    /// Trust bundle version
    pub version: u32,
    
    /// Creation timestamp
    pub created_at: SystemTime,
    
    /// List of trusted DIDs
    pub trusted_dids: Vec<String>,
    
    /// Issuer DID
    pub issuer: String,
    
    /// Trust bundle signature
    pub signature: Option<String>,
    
    /// Epoch number
    pub epoch: u64,
    
    /// Expiration timestamp
    pub expires_at: Option<SystemTime>,
    
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    
    /// Trust bundle attestations from validators
    #[serde(default)]
    pub attestations: HashMap<String, String>,
}

impl TrustBundle {
    /// Create a new trust bundle
    pub fn new(name: String, issuer: String, trusted_dids: Vec<String>) -> Self {
        Self {
            id: String::new(), // Will be set after CID generation
            name,
            version: 1,
            created_at: SystemTime::now(),
            trusted_dids,
            issuer,
            signature: None,
            epoch: 1,
            expires_at: None,
            metadata: HashMap::new(),
            attestations: HashMap::new(),
        }
    }
    
    /// Convert to a DAG node
    pub fn to_dag_node(&self) -> Result<DagNode, SyncError> {
        // Convert to JSON
        let value = serde_json::to_value(self)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Serialize to bytes for CID generation and payload
        let json_bytes = serde_json::to_vec(&value)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Generate CID using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(&json_bytes);
        let digest = hasher.finalize();
        let cid = format!("bafybeih{}", hex::encode(&digest[0..16]));
        
        // Create DAG node
        let node = DagNode {
            cid: cid.clone(),
            parents: Vec::new(),
            issuer: self.issuer.clone(),
            timestamp: self.created_at,
            signature: self.signature.clone().unwrap_or_default().into_bytes(),
            payload: json_bytes,
            metadata: DagNodeMetadata {
                sequence: Some(self.epoch),
                scope: Some("federation".to_string()),
            },
        };
        
        Ok(node)
    }
    
    /// Generate ID for this trust bundle
    pub fn generate_id(&mut self) -> Result<(), SyncError> {
        // Temporarily clear the ID for consistent CID generation
        self.id = String::new();
        
        // Convert to JSON
        let value = serde_json::to_value(self)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Serialize to bytes for CID generation
        let json_bytes = serde_json::to_vec(&value)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Generate CID using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(&json_bytes);
        let digest = hasher.finalize();
        let cid = format!("bafybeih{}", hex::encode(&digest[0..16]));
        
        // Set the ID
        self.id = cid;
        
        Ok(())
    }
    
    /// Check if the trust bundle is expired
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires) => {
                match SystemTime::now().duration_since(expires) {
                    Ok(_) => true, // Current time is after expiration
                    Err(_) => false, // Current time is before expiration
                }
            },
            None => false, // No expiration time set
        }
    }
    
    /// Count nodes by role
    pub fn count_nodes_by_role(&self, role: &str) -> usize {
        self.metadata.iter()
            .filter(|(k, v)| k.starts_with("role:") && v == role)
            .count()
    }
}

/// Trust bundle manager
pub struct TrustManager {
    /// Sync client
    client: SyncClient,
    
    /// Latest known trust bundle
    latest_bundle: Arc<Mutex<Option<TrustBundle>>>,
}

impl TrustManager {
    /// Create a new trust bundle manager
    pub fn new(client: SyncClient) -> Self {
        Self {
            client,
            latest_bundle: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Submit a trust bundle to the node
    pub async fn submit_trust_bundle(&self, trust_bundle: &mut TrustBundle) -> Result<String, SyncError> {
        // Generate ID if not set
        if trust_bundle.id.is_empty() {
            trust_bundle.generate_id()?;
        }
        
        // Convert to DAG node
        let node = trust_bundle.to_dag_node()?;
        
        // Submit the node
        let response = self.client.submit_node(&node).await?;
        
        // Update latest bundle if this is newer
        let mut latest = self.latest_bundle.lock().await;
        if let Some(ref current) = *latest {
            if trust_bundle.epoch > current.epoch {
                *latest = Some(trust_bundle.clone());
            }
        } else {
            *latest = Some(trust_bundle.clone());
        }
        
        Ok(response.id)
    }
    
    /// Get a trust bundle by ID
    pub async fn get_trust_bundle(&self, bundle_id: &str) -> Result<TrustBundle, SyncError> {
        // Get the DAG node
        let node = self.client.get_node(bundle_id).await?;
        
        // Parse the payload as JSON
        let payload_json = node.payload_as_json()
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Convert to TrustBundle
        let trust_bundle = serde_json::from_value(payload_json)
            .map_err(|e| SyncError::Serialization(e))?;
        
        Ok(trust_bundle)
    }
    
    /// Verify a DAG node against the latest trust bundle
    pub async fn verify_dag_node(&self, node: &DagNode) -> Result<bool, SyncError> {
        // Get the latest trust bundle
        let latest = self.latest_bundle.lock().await;
        
        // If no trust bundle is available, we can't verify
        let bundle = match *latest {
            Some(ref bundle) => bundle,
            None => return Err(SyncError::Validation("No trust bundle available for verification".to_string())),
        };
        
        // Check if the issuer is trusted
        if !bundle.trusted_dids.contains(&node.issuer) {
            return Err(SyncError::Validation(format!("Issuer {} is not trusted", node.issuer)));
        }
        
        // In a real implementation, we would verify the signature
        // For now, just return true if the issuer is trusted
        Ok(true)
    }
    
    /// Synchronize with the node to get the latest trust bundle
    pub async fn sync_trust_bundle(&self) -> Result<Option<TrustBundle>, SyncError> {
        // Get current epoch
        let current_epoch = {
            let latest = self.latest_bundle.lock().await;
            match *latest {
                Some(ref bundle) => bundle.epoch,
                None => 0,
            }
        };
        
        // Construct the URL for the latest trust bundle
        let url = format!("/api/v1/federation/trust-bundle/latest");
        
        // Request the latest trust bundle
        // In a real implementation, we would send a real request to the node
        // For now, just create a dummy trust bundle with a higher epoch
        
        // TODO: Replace with actual request to the node
        let dummy_bundle = TrustBundle {
            id: format!("trust-bundle-{}", current_epoch + 1),
            name: "Latest Trust Bundle".to_string(),
            version: 1,
            created_at: SystemTime::now(),
            trusted_dids: vec!["did:icn:trusted1".to_string(), "did:icn:trusted2".to_string()],
            issuer: "did:icn:federation".to_string(),
            signature: Some("dummy-signature".to_string()),
            epoch: current_epoch + 1,
            expires_at: None,
            metadata: HashMap::new(),
            attestations: HashMap::new(),
        };
        
        // Update latest bundle
        {
            let mut latest = self.latest_bundle.lock().await;
            *latest = Some(dummy_bundle.clone());
        }
        
        Ok(Some(dummy_bundle))
    }
    
    /// Get the latest known trust bundle
    pub async fn get_latest_trust_bundle(&self) -> Option<TrustBundle> {
        let latest = self.latest_bundle.lock().await;
        latest.clone()
    }
} 