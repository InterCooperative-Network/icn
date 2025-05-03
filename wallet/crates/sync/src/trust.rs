use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{SyncClient, error::SyncError, DagNode, generate_cid};

/// Trust bundle containing verified DIDs and credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    /// Trust bundle ID
    pub id: String,
    
    /// Trust bundle name
    pub name: String,
    
    /// Trust bundle version
    pub version: String,
    
    /// Creation timestamp
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    
    /// List of trusted DIDs
    pub trusted_dids: Vec<String>,
    
    /// Issuer DID
    pub issuer: String,
    
    /// Trust bundle signature
    pub signature: Option<String>,
}

impl TrustBundle {
    /// Create a new trust bundle
    pub fn new(name: String, issuer: String, trusted_dids: Vec<String>) -> Self {
        Self {
            id: String::new(), // Will be set after CID generation
            name,
            version: "1.0.0".to_string(),
            created_at: Utc::now(),
            trusted_dids,
            issuer,
            signature: None,
        }
    }
    
    /// Convert to a DAG node
    pub fn to_dag_node(&self) -> Result<DagNode, SyncError> {
        // Convert to JSON Value (compatible with runtime expectations)
        let data = serde_json::to_value(self)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Generate CID for the serialized JSON
        let json_bytes = serde_json::to_vec(self)
            .map_err(|e| SyncError::Serialization(e))?;
        let id = generate_cid(&json_bytes)?;
        
        // Create DAG node with timestamp from the trust bundle
        let node = DagNode {
            id: id.clone(),
            data,
            created_at: self.created_at,
            refs: Vec::new(),
        };
        
        Ok(node)
    }
    
    /// Generate CID for this trust bundle
    pub fn generate_id(&mut self) -> Result<(), SyncError> {
        // Temporarily clear the ID for consistent CID generation
        self.id = String::new();
        
        // Convert to bytes and generate CID
        let json_bytes = serde_json::to_vec(self)
            .map_err(|e| SyncError::Serialization(e))?;
        let id = generate_cid(&json_bytes)?;
        
        // Set the ID
        self.id = id;
        
        Ok(())
    }
}

/// Trust bundle manager
pub struct TrustManager {
    /// Sync client
    client: SyncClient,
}

impl TrustManager {
    /// Create a new trust bundle manager
    pub fn new(client: SyncClient) -> Self {
        Self {
            client,
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
        
        Ok(response.id)
    }
    
    /// Get a trust bundle by ID
    pub async fn get_trust_bundle(&self, bundle_id: &str) -> Result<TrustBundle, SyncError> {
        // Get the DAG node
        let node = self.client.get_node(bundle_id).await?;
        
        // Convert from JSON
        let trust_bundle = serde_json::from_value(node.data)
            .map_err(|e| SyncError::Serialization(e))?;
        
        Ok(trust_bundle)
    }
} 