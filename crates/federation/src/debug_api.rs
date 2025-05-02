//! Debug API module for testing and diagnostics
//!
//! This module provides read-only API endpoints specifically for integration testing
//! and debugging purposes. These endpoints are only active when the runtime is in
//! development or testing mode.

use async_trait::async_trait;
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
        // Look up the proposal in storage
        let dag_node = self.get_dag_node(proposal_cid).await?;
        
        if let Some(node) = dag_node {
            // Extract proposal information from DAG node
            let content_type = node.content_type.clone();
            let status = if content_type.contains("proposal-executed") {
                "executed"
            } else if content_type.contains("proposal-finalized") {
                "finalized"
            } else if content_type.contains("proposal") {
                "active"
            } else {
                "unknown"
            };
            
            // Count votes by looking for links from this node
            let vote_count = node.links.len() as u32;
            let executed = content_type.contains("executed");
            
            Ok(ProposalStatusResponse {
                exists: true,
                status: status.to_string(),
                created_at: Some(node.timestamp),
                finalized_at: if status == "finalized" || status == "executed" {
                    Some(node.timestamp)
                } else {
                    None
                },
                vote_count,
                executed,
            })
        } else {
            // Proposal not found
            Ok(ProposalStatusResponse {
                exists: false,
                status: "not_found".to_string(),
                created_at: None,
                finalized_at: None,
                vote_count: 0,
                executed: false,
            })
        }
    }
    
    async fn query_dag_node(&self, node_cid: &Cid) -> FederationResult<Option<DagNodeResponse>> {
        let dag_node = self.get_dag_node(node_cid).await?;
        
        if let Some(node) = dag_node {
            // Convert the DAG node to the response format
            let links = node.links.iter()
                .map(|link| link.to_string())
                .collect();
                
            let size = serde_json::to_vec(&node)
                .map_err(|e| FederationError::SerializationError(format!("Failed to serialize DAG node: {}", e)))?
                .len();
                
            Ok(Some(DagNodeResponse {
                cid: node_cid.to_string(),
                content_type: node.content_type,
                timestamp: node.timestamp,
                links,
                size,
            }))
        } else {
            Ok(None)
        }
    }
    
    async fn query_federation_status(&self) -> FederationResult<FederationStatusResponse> {
        // Get the current trust bundle to build the response
        let current_bundle = self.federation_manager.get_current_trust_bundle().await?;
        
        if let Some(bundle) = current_bundle {
            let mut validator_count = 0;
            let mut guardian_count = 0;
            let mut observer_count = 0;
            
            // Count node types
            for node in &bundle.nodes {
                match node.role.as_str() {
                    "Validator" => validator_count += 1,
                    "Guardian" => guardian_count += 1,
                    "Observer" => observer_count += 1,
                    _ => {}
                }
            }
            
            // Get connected peers count
            let peers = self.federation_manager.get_connected_peers().await?;
            
            Ok(FederationStatusResponse {
                current_epoch: bundle.epoch_id,
                node_count: bundle.nodes.len(),
                connected_peers: peers.len(),
                validator_count,
                guardian_count,
                observer_count,
            })
        } else {
            // No trust bundle available
            Ok(FederationStatusResponse {
                current_epoch: 0,
                node_count: 0,
                connected_peers: 0,
                validator_count: 0,
                guardian_count: 0,
                observer_count: 0,
            })
        }
    }
    
    async fn query_connected_peers(&self) -> FederationResult<Vec<String>> {
        self.federation_manager.get_connected_peers().await
    }
    
    async fn query_current_trust_bundle(&self) -> FederationResult<Option<TrustBundle>> {
        self.federation_manager.get_current_trust_bundle().await
    }
}

// Implementation to be used when runtime is in development or testing mode
#[cfg(any(debug_assertions, test, feature = "testing"))]
pub fn register_debug_api_routes(app: &mut impl http_server::Router, debug_api: Arc<dyn DebugApi>) {
    use http_server::{Method, Response};
    
    // Route for querying proposal status
    app.add_route("/api/v1/debug/proposal/:cid", Method::GET, move |req| {
        let debug_api = debug_api.clone();
        
        Box::pin(async move {
            let cid_str = match req.params.get("cid") {
                Some(c) => c,
                None => {
                    return Response::error("Missing proposal CID parameter", 400);
                }
            };
            
            let cid = match Cid::try_from(cid_str) {
                Ok(c) => c,
                Err(e) => {
                    return Response::error(&format!("Invalid CID format: {}", e), 400);
                }
            };
            
            match debug_api.query_proposal_status(&cid).await {
                Ok(status) => Response::json(&status),
                Err(e) => Response::error(&format!("Failed to query proposal: {}", e), 500),
            }
        })
    });
    
    // Route for querying DAG node information
    app.add_route("/api/v1/debug/dag/:cid", Method::GET, move |req| {
        let debug_api = debug_api.clone();
        
        Box::pin(async move {
            let cid_str = match req.params.get("cid") {
                Some(c) => c,
                None => {
                    return Response::error("Missing DAG node CID parameter", 400);
                }
            };
            
            let cid = match Cid::try_from(cid_str) {
                Ok(c) => c,
                Err(e) => {
                    return Response::error(&format!("Invalid CID format: {}", e), 400);
                }
            };
            
            match debug_api.query_dag_node(&cid).await {
                Ok(Some(node)) => Response::json(&node),
                Ok(None) => Response::error("DAG node not found", 404),
                Err(e) => Response::error(&format!("Failed to query DAG node: {}", e), 500),
            }
        })
    });
    
    // Route for querying federation status
    app.add_route("/api/v1/debug/federation/status", Method::GET, move |_req| {
        let debug_api = debug_api.clone();
        
        Box::pin(async move {
            match debug_api.query_federation_status().await {
                Ok(status) => Response::json(&status),
                Err(e) => Response::error(&format!("Failed to query federation status: {}", e), 500),
            }
        })
    });
    
    // Route for querying connected peers
    app.add_route("/api/v1/debug/federation/peers", Method::GET, move |_req| {
        let debug_api = debug_api.clone();
        
        Box::pin(async move {
            match debug_api.query_connected_peers().await {
                Ok(peers) => Response::json(&peers),
                Err(e) => Response::error(&format!("Failed to query connected peers: {}", e), 500),
            }
        })
    });
    
    // Route for querying current trust bundle
    app.add_route("/api/v1/debug/federation/trust-bundle", Method::GET, move |_req| {
        let debug_api = debug_api.clone();
        
        Box::pin(async move {
            match debug_api.query_current_trust_bundle().await {
                Ok(Some(bundle)) => Response::json(&bundle),
                Ok(None) => Response::error("No trust bundle available", 404),
                Err(e) => Response::error(&format!("Failed to query trust bundle: {}", e), 500),
            }
        })
    });
    
    // Add a convenience endpoint that returns all debugging endpoints
    app.add_route("/api/v1/debug", Method::GET, move |_req| {
        Box::pin(async move {
            let endpoints = HashMap::from([
                ("proposal_status", "/api/v1/debug/proposal/:cid"),
                ("dag_node", "/api/v1/debug/dag/:cid"),
                ("federation_status", "/api/v1/debug/federation/status"),
                ("connected_peers", "/api/v1/debug/federation/peers"),
                ("trust_bundle", "/api/v1/debug/federation/trust-bundle"),
            ]);
            
            Response::json(&endpoints)
        })
    });
} 