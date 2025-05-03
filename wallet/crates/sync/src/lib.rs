use std::time::SystemTime;
use std::str::FromStr;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use uuid::Uuid;
use reqwest::Client;
use tokio::sync::Mutex;
use backoff::{ExponentialBackoff, backoff::Backoff};

// Sub-modules
pub mod api;
pub mod trust;

// Re-export key types
pub use api::{FederationInfo, PeerInfo};
pub use trust::{TrustBundle, TrustManager};

// Re-export multihash to avoid version conflicts
pub mod compat {
    pub use multihash_0_16_3 as multihash;
}

/// Error type for synchronization operations
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("Node error: {0}")]
    NodeError(String),

    #[error("CID error: {0}")]
    CidError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Backoff error: operation timed out")]
    BackoffError,
}

// Custom conversion for backoff error handling
impl<E> From<backoff::Error<E>> for SyncError 
where 
    E: Into<SyncError> 
{
    fn from(err: backoff::Error<E>) -> Self {
        match err {
            backoff::Error::Permanent(e) => e.into(),
            backoff::Error::Transient(e) => e.into(),
        }
    }
}

/// DAG Node representation compatible with wallet and runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// Node identifier (CID)
    pub id: String, 
    
    /// Node data as JSON value
    pub data: Value,
    
    /// Creation timestamp
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    
    /// References to other nodes
    pub refs: Vec<String>,
}

impl DagNode {
    /// Create a new DAG node
    pub fn new(id: String, data: Value, refs: Vec<String>) -> Self {
        Self {
            id,
            data,
            created_at: Utc::now(),
            refs,
        }
    }
}

/// Response from node submission API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSubmissionResponse {
    /// Node ID (CID)
    pub id: String,
    
    /// Timestamp when the node was accepted
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    
    /// Block number (if applicable)
    pub block_number: Option<u64>,
    
    /// Node data
    pub data: Option<Value>,
}

/// Synchronization client for interacting with ICN nodes
#[derive(Clone)]
pub struct SyncClient {
    /// HTTP client
    client: Client,
    
    /// Base URL for ICN node
    base_url: String,
    
    /// Authentication token (if needed)
    auth_token: Option<String>,
}

impl SyncClient {
    /// Create a new synchronization client
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            auth_token: None,
        }
    }
    
    /// Set authentication token
    pub fn with_auth_token(mut self, token: String) -> Self {
        self.auth_token = Some(token);
        self
    }
    
    /// Submit a DAG node to the ICN node
    pub async fn submit_node(&self, node: &DagNode) -> Result<NodeSubmissionResponse, SyncError> {
        let url = format!("{}/api/v1/dag/nodes", self.base_url);
        
        let mut request = self.client.post(&url).json(node);
        
        // Add authentication if available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::NodeError(format!("Failed to submit node: HTTP {}: {}", status, error_text)));
        }
        
        let submission_response = response.json::<NodeSubmissionResponse>().await?;
        Ok(submission_response)
    }
    
    /// Get a DAG node by ID
    pub async fn get_node(&self, node_id: &str) -> Result<DagNode, SyncError> {
        let url = format!("{}/api/v1/dag/nodes/{}", self.base_url, node_id);
        
        let mut request = self.client.get(&url);
        
        // Add authentication if available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::NodeError(format!("Failed to get node: HTTP {}: {}", status, error_text)));
        }
        
        let node = response.json::<DagNode>().await?;
        Ok(node)
    }
}

/// Synchronization service for handling wallet data synchronization
pub struct SyncService {
    /// Sync client
    client: SyncClient,
    
    /// Retry configuration
    backoff: ExponentialBackoff,
}

impl SyncService {
    /// Create a new synchronization service
    pub fn new(client: SyncClient) -> Self {
        // Create default backoff configuration
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(std::time::Duration::from_secs(60)),
            ..ExponentialBackoff::default()
        };
        
        Self {
            client,
            backoff,
        }
    }
    
    /// Submit a node with automatic retries
    pub async fn submit_node_with_retry(&self, node: &DagNode) -> Result<NodeSubmissionResponse, SyncError> {
        let mut backoff = self.backoff.clone();
        let client = self.client.clone();
        let node = node.clone();
        
        let operation = || async {
            match client.submit_node(&node).await {
                Ok(response) => Ok(response),
                Err(e) => {
                    // Only retry on network errors
                    match &e {
                        SyncError::NetworkError(_) => Err(backoff::Error::transient(e)),
                        _ => Err(backoff::Error::permanent(e)),
                    }
                }
            }
        };
        
        let result = backoff::future::retry(backoff, operation).await?;
        Ok(result)
    }
}

/// Helper function to generate a CID using the compatible multihash
pub fn generate_cid(data: &[u8]) -> Result<String, SyncError> {
    use compat::multihash::{MultihashDigest, Code};
    
    // Create a multihash from the data
    let multihash = Code::Sha2_256.digest(data);
    
    // Convert to a hex string
    let hex_string = hex::encode(multihash.to_bytes());
    
    Ok(hex_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_generate_cid() {
        let data = "test data".as_bytes();
        let cid = generate_cid(data).unwrap();
        
        // CID should be a non-empty string
        assert!(!cid.is_empty());
        
        // CID should be deterministic for the same input
        let cid2 = generate_cid(data).unwrap();
        assert_eq!(cid, cid2);
    }
    
    #[test]
    fn test_dag_node_serialization() {
        let node = DagNode {
            id: "test-id".to_string(),
            data: json!({ "test": "value" }),
            created_at: Utc::now(),
            refs: vec!["ref1".to_string(), "ref2".to_string()],
        };
        
        // Convert to JSON and back
        let json = serde_json::to_string(&node).unwrap();
        let node2: DagNode = serde_json::from_str(&json).unwrap();
        
        // Fields should match
        assert_eq!(node.id, node2.id);
        assert_eq!(node.data, node2.data);
        assert_eq!(node.refs, node2.refs);
    }
    
    #[tokio::test]
    async fn test_trust_bundle_to_dag_node() {
        use crate::trust::TrustBundle;
        
        // Create a trust bundle
        let mut bundle = TrustBundle::new(
            "Test Bundle".to_string(),
            "did:icn:issuer".to_string(),
            vec!["did:icn:1".to_string(), "did:icn:2".to_string()],
        );
        
        // Generate ID
        bundle.generate_id().unwrap();
        
        // Convert to DAG node
        let node = bundle.to_dag_node().unwrap();
        
        // ID should match
        assert_eq!(bundle.id, node.id);
        
        // Convert back from JSON
        let bundle2: TrustBundle = serde_json::from_value(node.data).unwrap();
        
        // Fields should match
        assert_eq!(bundle.id, bundle2.id);
        assert_eq!(bundle.name, bundle2.name);
        assert_eq!(bundle.trusted_dids, bundle2.trusted_dids);
    }
} 