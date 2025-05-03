//! Debug API module for testing and diagnostics
//!
//! This module provides read-only API endpoints specifically for integration testing
//! and debugging purposes. These endpoints are only active when the runtime is in
//! development or testing mode.

use async_trait::async_trait;
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use futures::lock::Mutex;

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

#[cfg(feature = "testing")]
pub mod implementation {
    use super::*;
    use tracing::info;
    
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
    }
    
    #[async_trait]
    impl DebugApi for BasicDebugApi {
        async fn query_proposal_status(&self, _proposal_cid: &Cid) -> FederationResult<ProposalStatusResponse> {
            // Simple placeholder implementation
            Ok(ProposalStatusResponse {
                exists: false,
                status: "NotImplemented".to_string(),
                created_at: None,
                finalized_at: None,
                vote_count: 0,
                executed: false,
            })
        }
        
        async fn query_dag_node(&self, _node_cid: &Cid) -> FederationResult<Option<DagNodeResponse>> {
            // Simple placeholder implementation
            Ok(None)
        }
        
        async fn query_federation_status(&self) -> FederationResult<FederationStatusResponse> {
            // Simple placeholder implementation
            Ok(FederationStatusResponse {
                current_epoch: 0,
                node_count: 0,
                connected_peers: 0,
                validator_count: 0,
                guardian_count: 0,
                observer_count: 0,
            })
        }
        
        async fn query_connected_peers(&self) -> FederationResult<Vec<String>> {
            // Simple placeholder implementation
            Ok(Vec::new())
        }
        
        async fn query_current_trust_bundle(&self) -> FederationResult<Option<TrustBundle>> {
            // Simple placeholder implementation
            Ok(None)
        }
    }
    
    // Placeholder for registering HTTP routes
    pub fn register_debug_api_routes(debug_api: Arc<dyn DebugApi>) {
        // Placeholder implementation that logs instead of starting an HTTP server
        info!("Debug API registered (HTTP server disabled for now)");
        let _ = debug_api; // Avoid unused variable warning
    }
}

#[cfg(feature = "testing")]
pub use implementation::*; 