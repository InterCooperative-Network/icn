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

#[cfg(all(feature = "testing", feature = "axum"))]
pub mod axum_implementation {
    use super::*;
    use tracing::{info, error, debug};
    use axum::{
        extract::{Path, State},
        routing::get,
        Router, Json,
        http::StatusCode,
        response::{IntoResponse, Response},
    };
    use tower_http::cors::{CorsLayer, Any};
    use serde_json::json;
    
    // Error type for API responses
    #[derive(Debug)]
    struct ApiError {
        status: StatusCode,
        message: String,
    }
    
    impl IntoResponse for ApiError {
        fn into_response(self) -> Response {
            let body = Json(json!({
                "error": self.message
            }));
            
            (self.status, body).into_response()
        }
    }
    
    // Convert FederationError to ApiError
    impl From<FederationError> for ApiError {
        fn from(error: FederationError) -> Self {
            let status = match &error {
                FederationError::NetworkError(_) => StatusCode::BAD_GATEWAY,
                FederationError::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
                FederationError::TrustBundleError { kind, .. } => {
                    if *kind == crate::errors::TrustBundleErrorKind::NotFound {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::BAD_REQUEST
                    }
                },
                FederationError::AuthorizationError(_) => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            
            Self {
                status,
                message: error.to_string(),
            }
        }
    }

    // Register HTTP routes for debugging API
    pub fn register_debug_api_routes(debug_api: Arc<dyn DebugApi>) -> Router {
        // Create CORS middleware
        let cors = CorsLayer::new()
            .allow_methods(Any)
            .allow_origin(Any)
            .allow_headers(Any);
        
        // Create the Axum router with our routes
        let router = Router::new()
            // Federation status route
            .route("/api/debug/status", get(get_federation_status))
            
            // DAG node query route
            .route("/api/debug/dag/:cid", get(get_dag_node))
            
            // Proposal status route
            .route("/api/debug/proposal/:cid", get(get_proposal_status))
            
            // Connected peers route
            .route("/api/debug/peers", get(get_connected_peers))
            
            // Trust bundle route
            .route("/api/debug/trust-bundle", get(get_trust_bundle))
            
            // Add CORS middleware
            .layer(cors)
            
            // Add shared state
            .with_state(debug_api);
            
        info!("Debug API routes registered");
        
        router
    }
    
    // Handler for federation status endpoint
    async fn get_federation_status(
        State(debug_api): State<Arc<dyn DebugApi>>,
    ) -> Result<Json<FederationStatusResponse>, ApiError> {
        let status = debug_api.query_federation_status().await?;
        Ok(Json(status))
    }
    
    // Handler for DAG node endpoint
    async fn get_dag_node(
        State(debug_api): State<Arc<dyn DebugApi>>,
        Path(cid_str): Path<String>,
    ) -> Result<Json<Option<DagNodeResponse>>, ApiError> {
        // Parse the CID string
        let cid = match Cid::try_from(cid_str) {
            Ok(cid) => cid,
            Err(e) => {
                return Err(ApiError {
                    status: StatusCode::BAD_REQUEST,
                    message: format!("Invalid CID: {}", e),
                });
            }
        };
        
        let dag_node = debug_api.query_dag_node(&cid).await?;
        Ok(Json(dag_node))
    }
    
    // Handler for proposal status endpoint
    async fn get_proposal_status(
        State(debug_api): State<Arc<dyn DebugApi>>,
        Path(cid_str): Path<String>,
    ) -> Result<Json<ProposalStatusResponse>, ApiError> {
        // Parse the CID string
        let cid = match Cid::try_from(cid_str) {
            Ok(cid) => cid,
            Err(e) => {
                return Err(ApiError {
                    status: StatusCode::BAD_REQUEST,
                    message: format!("Invalid CID: {}", e),
                });
            }
        };
        
        let proposal_status = debug_api.query_proposal_status(&cid).await?;
        Ok(Json(proposal_status))
    }
    
    // Handler for connected peers endpoint
    async fn get_connected_peers(
        State(debug_api): State<Arc<dyn DebugApi>>,
    ) -> Result<Json<Vec<String>>, ApiError> {
        let peers = debug_api.query_connected_peers().await?;
        Ok(Json(peers))
    }
    
    // Handler for trust bundle endpoint
    async fn get_trust_bundle(
        State(debug_api): State<Arc<dyn DebugApi>>,
    ) -> Result<Json<Option<TrustBundle>>, ApiError> {
        let trust_bundle = debug_api.query_current_trust_bundle().await?;
        Ok(Json(trust_bundle))
    }
}

#[cfg(feature = "testing")]
pub mod implementation {
    use super::*;
    use tracing::{info, error, debug};
    use crate::{FederationManager, create_sha256_multihash};
    
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
        async fn query_proposal_status(&self, proposal_cid: &Cid) -> FederationResult<ProposalStatusResponse> {
            debug!("Querying proposal status for CID: {}", proposal_cid);
            
            // Create a key for the proposal, assuming it follows the pattern used in governance-kernel
            // Note: The key format might differ in the actual governance kernel implementation.
            // Adjust this if needed based on how proposals are actually stored.
            let key_str = format!("proposal::{}", proposal_cid);
            let storage_guard = self.storage.lock().await;
            
            // Create a CID for the key in storage (assuming dag-cbor for the key itself)
            let key_hash = create_sha256_multihash(key_str.as_bytes());
            let key_cid = Cid::new_v1(0x71, key_hash); // Use dag-cbor (0x71) or raw (0x55) as appropriate for keys
            
            // Try to retrieve the proposal from storage using get_kv
            match storage_guard.get_kv(&key_cid).await {
                Ok(Some(proposal_bytes)) => {
                    // Try to deserialize the proposal assuming JSON format
                    match serde_json::from_slice::<icn_governance_kernel::Proposal>(&proposal_bytes) {
                        Ok(proposal) => {
                            // Sum up the votes
                            let vote_count = (proposal.votes_for + proposal.votes_against + proposal.votes_abstain) as u32;
                            
                            // Check if it's executed
                            let executed = matches!(proposal.status, icn_governance_kernel::ProposalStatus::Executed);
                            
                            // Timestamps are not directly available in the proposal struct
                            // Use placeholder logic: created_at = None, finalized_at = voting_end_time if finalized/executed
                            let created_at = None; // Placeholder - Proposal struct lacks creation timestamp
                            let finalized_at = if matches!(proposal.status, icn_governance_kernel::ProposalStatus::Finalized | icn_governance_kernel::ProposalStatus::Executed) {
                                Some(proposal.voting_end_time)
                            } else {
                                None
                            };
                            
                            Ok(ProposalStatusResponse {
                                exists: true,
                                status: format!("{:?}", proposal.status),
                                created_at,
                                finalized_at,
                                vote_count,
                                executed,
                            })
                        },
                        Err(e) => {
                            error!("Failed to deserialize proposal from storage (key: {}): {}", key_cid, e);
                            Err(FederationError::StorageError(format!("Failed to deserialize proposal: {}", e)))
                        }
                    }
                },
                Ok(None) => {
                    // Proposal not found for the given key CID
                    debug!("Proposal not found in storage for key CID: {}", key_cid);
                    Ok(ProposalStatusResponse {
                        exists: false,
                        status: "NotFound".to_string(),
                        created_at: None,
                        finalized_at: None,
                        vote_count: 0,
                        executed: false,
                    })
                },
                Err(e) => {
                    error!("Storage error when querying proposal (key: {}): {}", key_cid, e);
                    Err(FederationError::StorageError(format!("Storage error querying proposal: {}", e)))
                }
            }
        }
        
        async fn query_dag_node(&self, node_cid: &Cid) -> FederationResult<Option<DagNodeResponse>> {
            debug!("Querying DAG node for CID: {}", node_cid);
            
            let storage_guard = self.storage.lock().await;
            
            // Attempt to retrieve the DAG node blob from storage using get_blob
            match storage_guard.get_blob(node_cid).await {
                Ok(Some(node_bytes)) => {
                    // Try to deserialize the DAG node using serde_json (matching DagStore implementation)
                    match serde_json::from_slice::<DagNode>(&node_bytes) {
                        Ok(node) => {
                            // Convert links (parents) to strings
                            let links: Vec<String> = node.parents.iter()
                                .map(|cid| cid.to_string())
                                .collect();
                            
                            // Determine content type - Assume JSON storage based on DagStore
                            let content_type = "application/json".to_string(); 
                            
                            Ok(Some(DagNodeResponse {
                                cid: node_cid.to_string(),
                                content_type,
                                timestamp: node.timestamp(), // Uses metadata.timestamp
                                links,
                                size: node_bytes.len(), // Use the size of the raw blob bytes stored
                            }))
                        },
                        Err(e) => {
                            error!("Failed to deserialize DAG node {} from JSON: {}", node_cid, e);
                            // Return error, indicating storage inconsistency or wrong format
                            Err(FederationError::StorageError(format!("Failed to deserialize DAG node {}: {}", node_cid, e)))
                        }
                    }
                },
                Ok(None) => {
                    // Node not found
                    debug!("DAG node {} not found in storage", node_cid);
                    Ok(None)
                },
                Err(e) => {
                    error!("Storage error when querying DAG node {}: {}", node_cid, e);
                    Err(FederationError::StorageError(format!("Storage error querying DAG node {}: {}", node_cid, e)))
                }
            }
        }
        
        async fn query_federation_status(&self) -> FederationResult<FederationStatusResponse> {
            debug!("Querying federation status");
            
            // Get the current epoch from the federation manager
            let current_epoch = self.federation_manager.get_latest_known_epoch().await?;
            
            // Get the current trust bundle to count nodes by role
            // This uses the method implemented below, which calls federation_manager.request_trust_bundle
            let current_trust_bundle_result = self.query_current_trust_bundle().await;
            
            let mut node_count = 0;
            let mut validator_count = 0;
            let mut guardian_count = 0;
            let mut observer_count = 0;

            match current_trust_bundle_result {
                Ok(Some(bundle)) => {
                    node_count = bundle.attestations.len();
                    // Use case-sensitive role names based on common practice, adjust if needed
                    validator_count = bundle.count_nodes_by_role("Validator"); 
                    guardian_count = bundle.count_nodes_by_role("Guardian");
                    observer_count = bundle.count_nodes_by_role("Observer");
                },
                Ok(None) => {
                    debug!("No current trust bundle found for epoch {}", current_epoch);
                    // Counts remain 0
                },
                Err(e) => {
                    // Log the error but continue, returning 0 counts for roles
                    error!("Failed to query current trust bundle: {}", e);
                    // Depending on requirements, could return Err here instead
                }
            }
            
            // Get connected peers count
            let connected_peers_result = self.query_connected_peers().await;
            let connected_peers = match connected_peers_result {
                Ok(peers) => peers.len(),
                Err(e) => {
                    error!("Failed to get connected peers: {}", e);
                    0 // Default to 0 if there's an error getting peers
                }
            };
            
            Ok(FederationStatusResponse {
                current_epoch,
                node_count,
                connected_peers,
                validator_count,
                guardian_count,
                observer_count,
            })
        }
        
        async fn query_connected_peers(&self) -> FederationResult<Vec<String>> {
            debug!("Querying connected peers");
            // Call the FederationManager's method to get connected peers
            // Ensure FederationManager has this method implemented and accessible
            self.federation_manager.get_connected_peers().await
        }
        
        async fn query_current_trust_bundle(&self) -> FederationResult<Option<TrustBundle>> {
            debug!("Querying current trust bundle");
            
            // Get the latest known epoch
            let current_epoch = self.federation_manager.get_latest_known_epoch().await?;
            
            if current_epoch == 0 {
                 debug!("Latest known epoch is 0, assuming no trust bundle available yet.");
                 return Ok(None);
            }
            
            // Request the trust bundle for the current epoch from the network/cache
            debug!("Requesting trust bundle for epoch {}", current_epoch);
            self.federation_manager.request_trust_bundle(current_epoch).await
        }
    }

    // Register HTTP routes for debugging API (non-Axum version)
    pub fn register_debug_api_routes(debug_api: Arc<dyn DebugApi>) {
        #[cfg(feature = "axum")]
        {
            // If axum is available, use it
            let _router = crate::debug_api::axum_implementation::register_debug_api_routes(debug_api);
            info!("Axum-based Debug API routes registered");
        }
        
        #[cfg(not(feature = "axum"))]
        {
            // If axum is not available, just log
            info!("Debug API registered (HTTP server disabled, axum feature not enabled)");
            let _ = debug_api; // Avoid unused variable warning
        }
    }
}

#[cfg(feature = "testing")]
pub use implementation::*; 