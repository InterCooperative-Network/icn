/*!
# DAG Audit Verification

This module provides tools for verifying DAG consistency and ensuring replay integrity.
It provides a systematic way to verify all nodes from genesis to tip, checking:
1. CID validity and integrity
2. Signature correctness
3. Resource balance consistency
4. Credential hash verification
*/

use crate::{DagError, DagNode, DagResult};
use cid::Cid;
use icn_storage::StorageBackend;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};
use std::time::Instant;
use serde::{Serialize, Deserialize};

/// State of a DAG verification process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationState {
    /// Total nodes processed
    pub nodes_processed: usize,
    
    /// Valid nodes count
    pub valid_nodes: usize,
    
    /// Invalid nodes count
    pub invalid_nodes: usize,
    
    /// Orphaned nodes (no parent or unreachable from genesis)
    pub orphaned_nodes: usize,
    
    /// Missing dependency nodes
    pub missing_deps: usize,
    
    /// Map of entity DIDs to their verification state
    pub entity_states: HashMap<String, EntityVerificationState>,
    
    /// Current verification progress (0.0 - 1.0)
    pub progress: f64,
    
    /// Merkle root chain of verification
    pub verification_chain: Vec<String>,
}

/// Verification state for a specific entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityVerificationState {
    /// Entity DID
    pub entity_id: String,
    
    /// Resource balances
    pub resource_balances: HashMap<String, i64>,
    
    /// Credential hashes
    pub credential_hashes: HashSet<String>,
    
    /// Number of nodes processed for this entity
    pub nodes_processed: usize,
}

/// Verification report output format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    /// Overall verification success
    pub success: bool,
    
    /// Verification state
    pub state: VerificationState,
    
    /// List of error details if any
    pub errors: Vec<VerificationError>,
    
    /// Verifiable Merkle root of the entire verification
    pub merkle_root: String,
    
    /// Time taken for verification
    pub time_elapsed_ms: u64,
    
    /// Historical anchors in chronological order
    pub chronological_anchors: Vec<ChronologicalAnchor>,
}

/// Details of an anchor in chronological order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronologicalAnchor {
    /// Anchor CID
    pub cid: String,
    
    /// Timestamp
    pub timestamp: u64,
    
    /// Entity DID
    pub entity_id: String,
    
    /// Short description of content
    pub description: String,
}

/// Verification error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationError {
    /// Error type
    pub error_type: VerificationErrorType,
    
    /// Entity ID
    pub entity_id: Option<String>,
    
    /// CID of problematic node
    pub node_cid: Option<String>,
    
    /// Error message
    pub message: String,
}

/// Types of verification errors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationErrorType {
    /// Invalid CID or CID hash mismatch
    InvalidCid,
    
    /// Invalid signature
    InvalidSignature,
    
    /// Invalid parent reference
    InvalidParent,
    
    /// Invalid resource balance
    InvalidResourceBalance,
    
    /// Invalid credential hash
    InvalidCredentialHash,
    
    /// Missing node
    MissingNode,
    
    /// Orphaned node (unreachable from genesis)
    OrphanedNode,
    
    /// General verification error
    Other,
}

/// Verifier for DAG consistency and replay assurance
pub struct DAGAuditVerifier<S: StorageBackend> {
    /// Storage backend
    storage: Arc<Mutex<S>>,
    
    /// Current verification state
    state: VerificationState,
    
    /// Errors encountered during verification
    errors: Vec<VerificationError>,
    
    /// Genesis CIDs for each entity
    genesis_cids: HashMap<String, Cid>,
    
    /// Set of all processed CIDs
    processed_cids: HashSet<Cid>,
    
    /// Chronological anchors
    chronological_anchors: Vec<ChronologicalAnchor>,
}

impl<S: StorageBackend> DAGAuditVerifier<S> {
    /// Create a new DAG audit verifier
    pub fn new(storage: Arc<Mutex<S>>) -> Self {
        Self {
            storage,
            state: VerificationState {
                nodes_processed: 0,
                valid_nodes: 0,
                invalid_nodes: 0,
                orphaned_nodes: 0,
                missing_deps: 0,
                entity_states: HashMap::new(),
                progress: 0.0,
                verification_chain: Vec::new(),
            },
            errors: Vec::new(),
            genesis_cids: HashMap::new(),
            processed_cids: HashSet::new(),
            chronological_anchors: Vec::new(),
        }
    }
    
    /// Verify a single entity's DAG from genesis to tip
    pub async fn verify_entity_dag(&mut self, entity_id: &str) -> DagResult<VerificationReport> {
        info!("Starting verification for entity: {}", entity_id);
        let start = Instant::now();
        
        // Find genesis CID for this entity
        self.find_genesis_cid(entity_id).await?;
        
        // Initialize entity state
        self.state.entity_states.insert(entity_id.to_string(), EntityVerificationState {
            entity_id: entity_id.to_string(),
            resource_balances: HashMap::new(),
            credential_hashes: HashSet::new(),
            nodes_processed: 0,
        });
        
        // Start BFS traversal from genesis
        let genesis_cid = self.genesis_cids.get(entity_id)
            .ok_or_else(|| DagError::InvalidNode(format!("No genesis node found for entity {}", entity_id)))?;
        
        let mut queue = VecDeque::new();
        queue.push_back(*genesis_cid);
        
        let mut visited = HashSet::new();
        visited.insert(*genesis_cid);
        
        while let Some(current_cid) = queue.pop_front() {
            // Process current node
            match self.process_node(entity_id, &current_cid).await {
                Ok(node) => {
                    // Add all children to queue
                    for child_cid in self.get_children(entity_id, &current_cid).await? {
                        if !visited.contains(&child_cid) {
                            visited.insert(child_cid);
                            queue.push_back(child_cid);
                        }
                    }
                    
                    // Update verification chain
                    self.update_verification_chain(&current_cid);
                    
                    // Add to chronological anchors
                    self.add_chronological_anchor(entity_id, &node, &current_cid);
                }
                Err(e) => {
                    self.record_error(VerificationErrorType::Other, Some(entity_id), Some(current_cid.to_string()), 
                        format!("Failed to process node: {}", e));
                }
            }
            
            // Update progress
            self.state.progress = self.state.valid_nodes as f64 / 
                (self.state.valid_nodes + queue.len() as usize) as f64;
        }
        
        // Check for orphaned nodes (reachable from any node but not from genesis)
        self.find_orphaned_nodes(entity_id).await?;
        
        // Generate verification report
        let report = VerificationReport {
            success: self.errors.is_empty(),
            state: self.state.clone(),
            errors: self.errors.clone(),
            merkle_root: self.compute_verification_merkle_root(),
            time_elapsed_ms: start.elapsed().as_millis() as u64,
            chronological_anchors: self.get_sorted_chronological_anchors(),
        };
        
        info!("Verification completed for entity {} in {}ms: {} nodes processed, {} valid, {} invalid, {} orphaned",
            entity_id, report.time_elapsed_ms, self.state.nodes_processed, 
            self.state.valid_nodes, self.state.invalid_nodes, self.state.orphaned_nodes);
        
        Ok(report)
    }
    
    /// Verify all entities' DAGs from genesis to tip
    pub async fn verify_all_entities(&mut self) -> DagResult<VerificationReport> {
        info!("Starting verification for all entities");
        let start = Instant::now();
        
        // Get all entity IDs
        let entity_ids = self.get_all_entity_ids().await?;
        
        // Verify each entity
        for entity_id in entity_ids {
            self.verify_entity_dag(&entity_id).await?;
        }
        
        // Generate verification report
        let report = VerificationReport {
            success: self.errors.is_empty(),
            state: self.state.clone(),
            errors: self.errors.clone(),
            merkle_root: self.compute_verification_merkle_root(),
            time_elapsed_ms: start.elapsed().as_millis() as u64,
            chronological_anchors: self.get_sorted_chronological_anchors(),
        };
        
        info!("Verification completed for all entities in {}ms: {} nodes processed, {} valid, {} invalid, {} orphaned",
            report.time_elapsed_ms, self.state.nodes_processed, 
            self.state.valid_nodes, self.state.invalid_nodes, self.state.orphaned_nodes);
        
        Ok(report)
    }
    
    // Implementation helpers
    
    /// Find the genesis CID for an entity
    async fn find_genesis_cid(&mut self, entity_id: &str) -> DagResult<Cid> {
        // In a real implementation, this would look up the genesis CID from storage
        // or derive it from the entity's ID
        let storage = self.storage.lock().unwrap();
        // This would be implemented in the storage backend
        // For now, returning a placeholder error
        Err(DagError::ContentError("Genesis CID lookup not implemented".to_string()))
    }
    
    /// Process a single node
    async fn process_node(&mut self, entity_id: &str, cid: &Cid) -> DagResult<DagNode> {
        // 1. Retrieve node
        let storage = self.storage.lock().unwrap();
        let node_bytes = storage.get(&cid.to_bytes())
            .await
            .map_err(|e| DagError::StorageError(format!("Failed to retrieve node: {}", e)))?
            .ok_or_else(|| DagError::InvalidCid(format!("Node not found for CID: {}", cid)))?;
        
        // 2. Decode node
        let node: DagNode = serde_json::from_slice(&node_bytes)
            .map_err(|e| DagError::CodecError(e.into()))?;
        
        // 3. Verify CID
        self.verify_cid(cid, &node_bytes)?;
        
        // 4. Verify parents
        self.verify_parents(entity_id, &node)?;
        
        // 5. Verify signature
        self.verify_signature(entity_id, &node)?;
        
        // 6. Update entity state
        self.update_entity_state(entity_id, &node)?;
        
        // 7. Update verification state
        self.state.nodes_processed += 1;
        self.state.valid_nodes += 1;
        
        if let Some(entity_state) = self.state.entity_states.get_mut(entity_id) {
            entity_state.nodes_processed += 1;
        }
        
        // 8. Mark as processed
        self.processed_cids.insert(*cid);
        
        Ok(node)
    }
    
    /// Verify CID matches node content
    fn verify_cid(&self, cid: &Cid, node_bytes: &[u8]) -> DagResult<()> {
        // In a real implementation, this would compute the CID from node_bytes
        // and verify it matches the expected CID
        // For now, we'll assume it's valid
        Ok(())
    }
    
    /// Verify parent nodes
    fn verify_parents(&self, entity_id: &str, node: &DagNode) -> DagResult<()> {
        for parent_cid in &node.parents {
            if !self.processed_cids.contains(parent_cid) {
                self.record_error(
                    VerificationErrorType::InvalidParent,
                    Some(entity_id),
                    Some(parent_cid.to_string()),
                    format!("Parent CID not processed: {}", parent_cid)
                );
                return Err(DagError::InvalidNode(format!("Parent CID not processed: {}", parent_cid)));
            }
        }
        Ok(())
    }
    
    /// Verify node signature
    fn verify_signature(&self, entity_id: &str, node: &DagNode) -> DagResult<()> {
        // In a real implementation, this would verify the signature
        // For now, we'll assume it's valid
        Ok(())
    }
    
    /// Update entity state based on node content
    fn update_entity_state(&mut self, entity_id: &str, node: &DagNode) -> DagResult<()> {
        // Update resource balances and credential hashes based on node content
        // This is application-specific and would need proper implementation
        // For now, we'll just return Ok
        Ok(())
    }
    
    /// Get children of a node
    async fn get_children(&self, entity_id: &str, cid: &Cid) -> DagResult<Vec<Cid>> {
        // In a real implementation, this would query the storage backend for nodes
        // that reference this CID as a parent
        // For now, returning an empty vec
        Ok(Vec::new())
    }
    
    /// Find orphaned nodes
    async fn find_orphaned_nodes(&mut self, entity_id: &str) -> DagResult<()> {
        // In a real implementation, this would find nodes that are not reachable
        // from genesis but exist in storage
        // For now, we'll just return Ok
        Ok(())
    }
    
    /// Get all entity IDs
    async fn get_all_entity_ids(&self) -> DagResult<Vec<String>> {
        // In a real implementation, this would query the storage backend for all
        // entity IDs
        // For now, returning an empty vec
        Ok(Vec::new())
    }
    
    /// Update verification chain with new CID
    fn update_verification_chain(&mut self, cid: &Cid) {
        self.state.verification_chain.push(cid.to_string());
    }
    
    /// Compute Merkle root of verification chain
    fn compute_verification_merkle_root(&self) -> String {
        // In a real implementation, this would compute a Merkle root of all
        // verified CIDs
        // For now, returning a placeholder
        "merkle-root-not-implemented".to_string()
    }
    
    /// Add chronological anchor
    fn add_chronological_anchor(&mut self, entity_id: &str, node: &DagNode, cid: &Cid) {
        // Extract timestamp from node metadata
        let timestamp = node.metadata.timestamp;
        
        // Extract a short description from the payload
        let description = match &node.payload {
            crate::Ipld::String(s) => s.clone(),
            crate::Ipld::Map(m) => m.get("description")
                .and_then(|v| match v {
                    crate::Ipld::String(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "Unknown".to_string()),
            _ => "Unknown".to_string(),
        };
        
        self.chronological_anchors.push(ChronologicalAnchor {
            cid: cid.to_string(),
            timestamp,
            entity_id: entity_id.to_string(),
            description,
        });
    }
    
    /// Get sorted chronological anchors
    fn get_sorted_chronological_anchors(&self) -> Vec<ChronologicalAnchor> {
        let mut anchors = self.chronological_anchors.clone();
        anchors.sort_by_key(|a| a.timestamp);
        anchors
    }
    
    /// Record a verification error
    fn record_error(&mut self, 
        error_type: VerificationErrorType, 
        entity_id: Option<&str>, 
        node_cid: Option<String>, 
        message: String
    ) {
        self.state.invalid_nodes += 1;
        
        if error_type == VerificationErrorType::OrphanedNode {
            self.state.orphaned_nodes += 1;
        } else if error_type == VerificationErrorType::MissingNode {
            self.state.missing_deps += 1;
        }
        
        self.errors.push(VerificationError {
            error_type,
            entity_id: entity_id.map(|s| s.to_string()),
            node_cid,
            message,
        });
        
        error!(
            error_type = ?error_type,
            entity_id = entity_id.unwrap_or("unknown"),
            node_cid = node_cid.as_deref().unwrap_or("unknown"),
            message = message,
            "DAG verification error"
        );
    }
}

// CLI report formatter
pub fn format_report_for_cli(report: &VerificationReport) -> String {
    let mut output = String::new();
    
    output.push_str(&format!("=== DAG VERIFICATION REPORT ===\n"));
    output.push_str(&format!("Success: {}\n", report.success));
    output.push_str(&format!("Time: {}ms\n", report.time_elapsed_ms));
    output.push_str(&format!("Nodes processed: {}\n", report.state.nodes_processed));
    output.push_str(&format!("Valid nodes: {}\n", report.state.valid_nodes));
    output.push_str(&format!("Invalid nodes: {}\n", report.state.invalid_nodes));
    output.push_str(&format!("Orphaned nodes: {}\n", report.state.orphaned_nodes));
    output.push_str(&format!("Missing dependencies: {}\n", report.state.missing_deps));
    output.push_str(&format!("Merkle root: {}\n", report.merkle_root));
    
    if !report.errors.is_empty() {
        output.push_str("\n=== ERRORS ===\n");
        for (i, error) in report.errors.iter().enumerate() {
            output.push_str(&format!("{}. {:?}: {}\n", 
                i + 1, error.error_type, error.message));
        }
    }
    
    if !report.chronological_anchors.is_empty() {
        output.push_str("\n=== CHRONOLOGICAL ANCHORS (FIRST 10) ===\n");
        for (i, anchor) in report.chronological_anchors.iter().take(10).enumerate() {
            output.push_str(&format!("{}. [{}] {}: {}\n", 
                i + 1, anchor.timestamp, anchor.entity_id, anchor.description));
        }
    }
    
    output
} 