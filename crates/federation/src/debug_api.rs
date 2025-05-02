//! Debug API module for testing and diagnostics
//!
//! This module provides read-only API endpoints specifically for integration testing
//! and debugging purposes. These endpoints are only active when the runtime is in
//! development or testing mode.

use async_trait::async_trait;
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::errors::{FederationError, FederationResult};
use icn_dag::DagNode;
use icn_identity::TrustBundle;
use icn_storage::StorageBackend;

/// Debug query response for proposal status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalStatusResponse {
    /// Whether the proposal exists in the system
    pub exists: bool,
    /// Current status of the proposal
    pub status: String,
    /// Timestamp when the proposal was created (if available)
    pub created_at: Option<i64>,
    /// Timestamp when the proposal was finalized (if available)
    pub finalized_at: Option<i64>,
    /// Number of votes cast on this proposal
    pub vote_count: u32,
    /// Whether the proposal has been executed
    pub executed: bool,
}

/// Debug query response for DAG node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNodeResponse {
    /// The CID of the DAG node
    pub cid: String,
    /// DAG node content type
    pub content_type: String,
    /// Timestamp when the DAG node was created
    pub timestamp: i64,
    /// Links to other DAG nodes
    pub links: Vec<String>,
    /// Size of the DAG node content in bytes
    pub size: usize,
}

/// Debug query response for federation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationStatusResponse {
    /// Current epoch number
    pub current_epoch: u64,
    /// Node count in the current trust bundle
    pub node_count: usize,
    /// Connected peer count
    pub connected_peers: usize,
    /// Validator count
    pub validator_count: usize,
    /// Guardian count
    pub guardian_count: usize,
    /// Observer count
    pub observer_count: usize,
}

/// Debug API trait defining the operations available for testing and diagnostics
#[async_trait]
pub trait DebugApi: Send + Sync {
    /// Query the status of a proposal by its CID
    async fn query_proposal_status(&self, proposal_cid: &Cid) -> FederationResult<ProposalStatusResponse>;
    
    /// Query information about a DAG node by its CID
    async fn query_dag_node(&self, node_cid: &Cid) -> FederationResult<Option<DagNodeResponse>>;
    
    /// Query the current federation status
    async fn query_federation_status(&self) -> FederationResult<FederationStatusResponse>;
    
    /// Get list of peers the node is connected to
    async fn query_connected_peers(&self) -> FederationResult<Vec<String>>;
    
    /// Get the current trust bundle
    async fn query_current_trust_bundle(&self) -> FederationResult<Option<TrustBundle>>;
}

// Implementation of the trait and HTTP routing is only compiled when testing feature is enabled
#[cfg(feature = "testing")]
mod implementation {
    use super::*;
    use std::collections::HashMap;

    /// Basic implementation of the Debug API
    pub struct BasicDebugApi {
        storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>,
        federation_manager: Arc<crate::FederationManager>,
    }

    impl BasicDebugApi {
        /// Create a new instance of the basic debug API implementation
        pub fn new(
            storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>,
            federation_manager: Arc<crate::FederationManager>,
        ) -> Self {
            Self {
                storage,
                federation_manager,
            }
        }
        
        /// Helper method to get a DAG node from storage
        async fn get_dag_node(&self, cid: &Cid) -> FederationResult<Option<DagNode>> {
            let storage_lock = self.storage.lock().await;
            let node_bytes = storage_lock.get_blob(cid).await
                .map_err(|e| FederationError::StorageError(format!("Failed to get DAG node: {}", e)))?;
            
            if let Some(bytes) = node_bytes {
                let node: DagNode = serde_json::from_slice(&bytes)
                    .map_err(|e| FederationError::SerializationError(format!("Failed to deserialize DAG node: {}", e)))?;
                Ok(Some(node))
            } else {
                Ok(None)
            }
        }
    }

    #[async_trait]
    impl DebugApi for BasicDebugApi {
        async fn query_proposal_status(&self, proposal_cid: &Cid) -> FederationResult<ProposalStatusResponse> {
            todo!("This API is only available when the testing feature is enabled")
        }
        
        async fn query_dag_node(&self, node_cid: &Cid) -> FederationResult<Option<DagNodeResponse>> {
            todo!("This API is only available when the testing feature is enabled")
        }
        
        async fn query_federation_status(&self) -> FederationResult<FederationStatusResponse> {
            todo!("This API is only available when the testing feature is enabled")
        }
        
        async fn query_connected_peers(&self) -> FederationResult<Vec<String>> {
            todo!("This API is only available when the testing feature is enabled")
        }
        
        async fn query_current_trust_bundle(&self) -> FederationResult<Option<TrustBundle>> {
            todo!("This API is only available when the testing feature is enabled")
        }
    }

    // This function would register HTTP routes when a proper HTTP server is available
    #[cfg(any(debug_assertions, test, feature = "testing"))]
    pub fn register_debug_api_routes(debug_api: Arc<dyn DebugApi>) {
        // This function is a stub until a proper HTTP server implementation is added
        let _ = debug_api;
    }
}

// Re-export the implementation when testing feature is enabled
#[cfg(feature = "testing")]
pub use implementation::*; 