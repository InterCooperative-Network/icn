use std::time::SystemTime;

use tracing::warn;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use reqwest::Client;
use backoff::ExponentialBackoff;
use futures::future::TryFutureExt;
use backoff::future::retry_notify;

// Sub-modules
pub mod api;
pub mod trust;
pub mod error;
pub mod federation;
pub mod compat;

// Re-export key types
pub use api::{FederationInfo, PeerInfo};
pub use trust::{TrustBundle, TrustManager};
pub use error::SyncError;
pub use federation::{FederationSyncClient, TrustBundleSubscription, FederationNodeAddress};
pub use compat::{LegacyDagNode, legacy_to_current, current_to_legacy, parse_dag_node_json};
pub use compat::{to_wallet_types_dag_node, from_wallet_types_dag_node};

// Define our own DagNodeMetadata to avoid circular dependencies
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DagNodeMetadata {
    pub sequence: Option<u64>,
    pub scope: Option<String>,
}

impl DagNodeMetadata {
    pub fn get(&self, key: &str) -> Option<&Value> {
        None // Placeholder implementation
    }
}

// Define our own DagNode structure to match what we need
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub cid: String,
    pub parents: Vec<String>,
    pub timestamp: SystemTime,
    pub creator: String,     // Used as issuer
    pub content: Vec<u8>,    // Used as payload
    pub content_type: String,
    pub signatures: Vec<String>,
    pub metadata: DagNodeMetadata,
}

impl DagNode {
    // Helper method for compatibility
    pub fn content_as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_slice(&self.content)
    }
    
    // For backward compatibility with existing code
    pub fn payload_as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_slice(&self.content)
    }
    
    // Return creator as Option<&str> for future-proofing
    pub fn get_issuer(&self) -> Option<&str> {
        if !self.creator.is_empty() {
            Some(&self.creator)
        } else {
            None
        }
    }
}

// Re-export only types that exist in wallet-types
pub use icn_wallet_types::NodeSubmissionResponse;

// Define a local version as needed
pub type WalletResult<T> = Result<T, error::SyncError>;

/// Synchronization client for interacting with ICN nodes
#[derive(Clone)]
pub struct SyncClient {
    /// HTTP client
    client: Client,
    
    /// Base URL for ICN node
    pub(crate) base_url: String,
    
    /// Authentication token (if needed)
    pub(crate) auth_token: Option<String>,
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
    
    /// Add authentication token for requests
    pub fn with_auth_token(mut self, token: String) -> Self {
        self.auth_token = Some(token);
        self
    }
    
    /// Submit a DAG node to the network
    pub async fn submit_node(&self, node: &DagNode) -> Result<NodeSubmissionResponse, SyncError> {
        let url = format!("{}/api/v1/dag", self.base_url);
        
        // Convert current node to legacy format for API compatibility
        let legacy_node = compat::current_to_legacy(node);
        
        let mut request = self.client.post(&url)
            .json(&legacy_node);
        
        // Add authentication if available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Api(format!("Failed to submit node: HTTP {}: {}", status, error_text)));
        }
        
        let submission_response = response.json::<NodeSubmissionResponse>().await?;
        Ok(submission_response)
    }
    
    /// Get a DAG node by ID
    pub async fn get_node(&self, node_id: &str) -> Result<DagNode, SyncError> {
        let url = format!("{}/api/v1/dag/{}", self.base_url, node_id);
        
        let mut request = self.client.get(&url);
        
        // Add authentication if available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Api(format!("Failed to get node: HTTP {}: {}", status, error_text)));
        }
        
        let json_value = response.json::<Value>().await?;
        
        // Use compatibility layer to parse response
        let node = compat::parse_dag_node_json(json_value)?;
        
        Ok(node)
    }
    
    /// Get a DagNode from icn-wallet-types and convert it to the sync format
    pub async fn get_node_as_wallet_type(&self, node_id: &str) -> Result<icn_wallet_types::dag::DagNode, SyncError> {
        let node = self.get_node(node_id).await?;
        compat::to_wallet_types_dag_node(&node)
    }
    
    /// Submit a DagNode in icn-wallet-types format
    pub async fn submit_wallet_type_node(&self, node: &icn_wallet_types::dag::DagNode) -> Result<NodeSubmissionResponse, SyncError> {
        let sync_node = compat::from_wallet_types_dag_node(node);
        self.submit_node(&sync_node).await
    }
    
    /// Extract thread_id from Execution Receipt credential
    pub fn extract_thread_id_from_credential(&self, credential_json: &Value) -> Option<String> {
        credential_json
            .get("credentialSubject")
            .and_then(|subject| subject.get("thread_id"))
            .and_then(|thread_id| thread_id.as_str())
            .map(|s| s.to_string())
    }
    
    /// Sync updated proposals with AgoraNet thread links
    pub async fn sync_proposal_with_thread(&self, proposal_id: &str) -> Result<Option<String>, SyncError> {
        // First get the proposal node
        let proposal_node = self.get_node(proposal_id).await?;
        
        // Check if the proposal has a credential
        if let Some(credential_refs) = proposal_node.metadata.get("credentials") {
            if let Some(credential_list) = credential_refs.as_array() {
                // Iterate through credentials to find execution receipts
                for cred_ref in credential_list {
                    if let Some(cred_id) = cred_ref.as_str() {
                        // Get the credential node
                        let credential_node = self.get_node(cred_id).await?;
                        
                        // Parse the credential JSON
                        if let Ok(credential_json) = serde_json::from_slice::<Value>(&credential_node.content) {
                            // Check if it's an execution receipt with thread_id
                            if let Some(thread_id) = self.extract_thread_id_from_credential(&credential_json) {
                                return Ok(Some(thread_id));
                            }
                        }
                    }
                }
            }
        }
        
        // No thread_id found
        Ok(None)
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
        let backoff = self.backoff.clone();
        let client = self.client.clone();
        let node = node.clone();
        
        let operation = || async {
            match client.submit_node(&node).await {
                Ok(response) => Ok(response),
                Err(e) => {
                    // Only retry on network errors
                    match &e {
                        SyncError::Network(_) => Err(backoff::Error::transient(e)),
                        SyncError::Request(_) if e.to_string().contains("timeout") => Err(backoff::Error::transient(e)),
                        _ => Err(backoff::Error::permanent(e)),
                    }
                }
            }
        };
        
        let result = retry_notify(backoff, operation, |err, dur| {
            warn!("Retrying after {:?} due to error: {}", dur, err);
        }).await?;
        Ok(result)
    }
}

/// Sync manager to coordinate synchronization
pub struct SyncManager {
    client: SyncClient,
    service: SyncService,
    trust_manager: Option<TrustManager>,
}

impl SyncManager {
    /// Create a new sync manager
    pub fn new(node_url: String) -> Self {
        let client = SyncClient::new(node_url);
        let service = SyncService::new(client.clone());
        
        Self {
            client,
            service,
            trust_manager: None,
        }
    }
    
    /// With authentication token
    pub fn with_auth_token(mut self, token: String) -> Self {
        self.client = self.client.with_auth_token(token);
        self
    }
    
    /// With trust manager for validating DAG nodes and TrustBundles
    pub fn with_trust_manager(mut self, trust_manager: TrustManager) -> Self {
        self.trust_manager = Some(trust_manager);
        self
    }
    
    /// Submit a DAG node to the network
    pub async fn submit_node(&self, node: &DagNode) -> Result<NodeSubmissionResponse, SyncError> {
        // Verify the node if we have a trust manager
        if let Some(trust_manager) = &self.trust_manager {
            trust_manager.verify_dag_node(node).await?;
        }
        
        // Submit the node with retry
        self.service.submit_node_with_retry(node).await
    }
    
    /// Get a DAG node by ID
    pub async fn get_node(&self, node_id: &str) -> Result<DagNode, SyncError> {
        let node = self.client.get_node(node_id).await?;
        
        // Verify the node if we have a trust manager
        if let Some(trust_manager) = &self.trust_manager {
            trust_manager.verify_dag_node(&node).await?;
        }
        
        Ok(node)
    }
    
    /// Synchronize with the node to get the latest trust bundle
    pub async fn sync_trust_bundle(&self) -> Result<Option<TrustBundle>, SyncError> {
        if let Some(trust_manager) = &self.trust_manager {
            trust_manager.sync_trust_bundle().await
        } else {
            Err(SyncError::Internal("No trust manager configured".to_string()))
        }
    }
}

/// Helper function to generate a CID using SHA-256
pub fn generate_cid(data: &[u8]) -> Result<String, SyncError> {
    use sha2::{Sha256, Digest};
    
    // Create a SHA-256 hash of the data
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    
    // Convert to a base58 string prefixed with 'bafybeih'
    let hex_string = format!("bafybeih{}", hex::encode(&hash[0..16]));
    
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
        // Create a DagNode using the current structure
        let node = DagNode {
            cid: "test-cid".to_string(),
            parents: vec!["ref1".to_string(), "ref2".to_string()],
            timestamp: SystemTime::now(),
            creator: "did:icn:test".to_string(),
            content: serde_json::to_vec(&json!({ "test": "value" })).unwrap(),
            content_type: "application/json".to_string(),
            signatures: vec![],
            metadata: DagNodeMetadata {
                sequence: Some(1),
                scope: Some("test".to_string()),
            },
        };
        
        // Convert to legacy format and back
        let legacy = compat::current_to_legacy(&node);
        let node2 = compat::legacy_to_current(&legacy);
        
        // Fields should match
        assert_eq!(node.cid, node2.cid);
        assert_eq!(node.parents, node2.parents);
        assert_eq!(node.content, node2.content);
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
        
        // CID should match bundle ID
        assert_eq!(bundle.id, node.cid);
        
        // Convert back from payload
        let payload_json = node.payload_as_json().unwrap();
        let bundle2: TrustBundle = serde_json::from_value(payload_json).unwrap();
        
        // Fields should match
        assert_eq!(bundle.id, bundle2.id);
        assert_eq!(bundle.name, bundle2.name);
        assert_eq!(bundle.trusted_dids, bundle2.trusted_dids);
    }
} 
