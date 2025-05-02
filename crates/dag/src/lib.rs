/*!
# ICN DAG System

This crate implements the Directed Acyclic Graph (DAG) system for the ICN Runtime.
It provides structures for representing DAG nodes, calculating Merkle roots, and
verifying lineage attestations.

## Architectural Tenets
- All state lives in append-only Merkle-anchored DAG objects; forkless by design
- Lineage attestations provide verifiable history
- Content addressing enables integrity verification
*/

use cid::Cid;
use icn_identity::{IdentityId, Signature};
use multihash::{self, Code, MultihashDigest};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use std::sync::{Arc, Mutex};
use icn_storage::StorageBackend;
use serde::{Serialize, Deserialize};
use serde_json;

/// Errors that can occur during DAG operations
#[derive(Debug, Error)]
pub enum DagError {
    #[error("Invalid DAG node: {0}")]
    InvalidNode(String),
    
    #[error("Merkle verification failed: {0}")]
    MerkleVerificationFailed(String),
    
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Content error: {0}")]
    ContentError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}

/// Result type for DAG operations
pub type DagResult<T> = Result<T, DagError>;

/// Metadata for a DAG node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNodeMetadata {
    /// Timestamp of when this node was created (unix timestamp in seconds)
    pub timestamp: u64,
    
    /// Sequence number of this node (optional)
    pub sequence: Option<u64>,
    
    /// Scope of this node (optional)
    pub scope: Option<String>,
}

impl DagNodeMetadata {
    /// Create a new metadata with current timestamp
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            sequence: None,
            scope: None,
        }
    }
    
    /// Create a new metadata with specified timestamp
    pub fn with_timestamp(timestamp: u64) -> Self {
        Self {
            timestamp,
            sequence: None,
            scope: None,
        }
    }
    
    /// Set the sequence number
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = Some(sequence);
        self
    }
    
    /// Set the scope
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

impl Default for DagNodeMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a node in the DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// Content identifier of this node
    pub cid: Option<Cid>,
    
    /// Content of this node
    pub content: Vec<u8>,
    
    /// Parent CIDs of this node
    pub parents: Vec<Cid>,
    
    /// Identity that signed this node
    pub signer: IdentityId,
    
    /// Signature of this node
    pub signature: Signature,
    
    /// Metadata of this node
    pub metadata: DagNodeMetadata,
}

impl DagNode {
    /// Create a new DAG node
    pub fn new(
        content: Vec<u8>,
        parents: Vec<Cid>,
        signer: IdentityId,
        signature: Signature,
        metadata: Option<DagNodeMetadata>,
    ) -> DagResult<Self> {
        let metadata = metadata.unwrap_or_default();
        
        let mut node = Self {
            cid: None,
            content,
            parents,
            signer,
            signature,
            metadata,
        };
        
        // Calculate and set the CID based on content
        node.calculate_cid()?;
        
        Ok(node)
    }
    
    /// Calculate the CID of this node based on its content
    pub fn calculate_cid(&mut self) -> DagResult<Cid> {
        // For simplicity, just use the content hash with SHA-256
        let mh = Code::Sha2_256.digest(&self.content);
        
        // Create CID with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| DagError::InvalidCid(e.to_string()))?;
        
        // Set the CID
        self.cid = Some(cid);
        
        // Return the CID
        self.cid.ok_or_else(|| DagError::InvalidCid("Failed to calculate CID".to_string()))
    }
    
    /// Verify the signature of this node
    pub fn verify_signature(&self) -> DagResult<()> {
        // Placeholder implementation that will be properly implemented later
        // For now, we'll assume it's valid if the signature is not empty
        if self.signature.0.is_empty() {
            return Err(DagError::SignatureVerificationFailed);
        }
        Ok(())
    }
    
    /// Get the CID of this node
    pub fn cid(&self) -> DagResult<Cid> {
        self.cid.ok_or_else(|| DagError::InvalidCid("CID not calculated".to_string()))
    }
}

/// Represents a lineage attestation for a DAG node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageAttestation {
    /// Root CID of the DAG
    pub root_cid: Cid,
    
    /// CID of the attested node
    pub node_cid: Cid,
    
    /// Merkle proof of inclusion
    pub proof: Vec<Vec<u8>>,
    
    /// Identity that signed this attestation
    pub signer: IdentityId,
    
    /// Signature of this attestation
    pub signature: Signature,
    
    /// Timestamp of when this attestation was created
    pub timestamp: u64,
}

impl LineageAttestation {
    /// Create a new lineage attestation
    pub fn new(
        root_cid: Cid,
        node_cid: Cid,
        proof: Vec<Vec<u8>>,
        signer: IdentityId,
        signature: Signature,
        timestamp: u64,
    ) -> DagResult<Self> {
        if proof.is_empty() {
            return Err(DagError::InvalidNode("Proof cannot be empty".to_string()));
        }
        
        Ok(Self {
            root_cid,
            node_cid,
            proof,
            signer,
            signature,
            timestamp,
        })
    }
    
    /// Verify the lineage attestation
    pub fn verify(&self) -> DagResult<()> {
        // Verify signature
        if self.signature.0.is_empty() {
            return Err(DagError::SignatureVerificationFailed);
        }
        
        // Verify proof (placeholder implementation)
        // The actual implementation will verify the Merkle proof
        if self.proof.is_empty() {
            return Err(DagError::MerkleVerificationFailed("Empty proof".to_string()));
        }
        
        Ok(())
    }
}

/// Calculates a Merkle root for a set of DAG nodes
pub fn calculate_merkle_root(nodes: &[DagNode]) -> DagResult<Cid> {
    if nodes.is_empty() {
        return Err(DagError::InvalidNode("Empty node list".to_string()));
    }
    
    // Extract CIDs of nodes as bytes
    let cids: Vec<Vec<u8>> = nodes
        .iter()
        .map(|node| {
            node.cid()
                .map(|cid| cid.to_bytes())
                .map_err(|_| DagError::InvalidCid("Node has no CID".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    
    // Simplified merkle root calculation - just hash all CIDs together
    // In a real implementation, this would use a proper Merkle tree
    let mut combined = Vec::new();
    for cid_bytes in cids {
        combined.extend_from_slice(&cid_bytes);
    }
    
    let mh = Code::Sha2_256.digest(&combined);
    
    let cid = Cid::new_v0(mh)
        .map_err(|e| DagError::InvalidCid(e.to_string()))?;
    
    Ok(cid)
}

/// Verifies a Merkle proof
pub fn verify_merkle_proof(
    _root: &[u8],
    proof: &[Vec<u8>],
    leaf: &[u8],
) -> DagResult<bool> {
    if proof.is_empty() {
        return Err(DagError::MerkleVerificationFailed("Empty proof".to_string()));
    }
    
    // Placeholder implementation that will be properly implemented later
    // For now, just return a basic check that proof and leaf are not empty
    if leaf.is_empty() {
        return Err(DagError::MerkleVerificationFailed("Empty leaf".to_string()));
    }
    
    // In a real implementation, this would verify the Merkle proof against the root
    
    Ok(true)
}

// Add the cache module
pub mod cache;

// Import cache
use cache::DagNodeCache;

// Update DagStore to use caching
pub struct DagStore {
    storage: Arc<Mutex<dyn StorageBackend>>,
    cache: DagNodeCache,
    config: DagStoreConfig,
}

// Add configuration options for the DagStore
#[derive(Clone, Debug)]
pub struct DagStoreConfig {
    /// Cache capacity (number of nodes)
    pub cache_capacity: usize,
    /// Whether to disable caching
    pub disable_cache: bool,
}

impl Default for DagStoreConfig {
    fn default() -> Self {
        Self {
            cache_capacity: 1000, // Default to caching 1000 nodes
            disable_cache: false,
        }
    }
}

impl DagStore {
    /// Create a new DAG store with default configuration
    pub fn new(storage: Arc<Mutex<dyn StorageBackend>>) -> Self {
        Self::with_config(storage, DagStoreConfig::default())
    }
    
    /// Create a new DAG store with custom configuration
    pub fn with_config(storage: Arc<Mutex<dyn StorageBackend>>, config: DagStoreConfig) -> Self {
        let cache = DagNodeCache::new(config.cache_capacity);
        Self { storage, cache, config }
    }
    
    /// Store a node in the DAG
    pub async fn store_node(&self, node: &DagNode) -> Result<Cid, DagError> {
        // Calculate CID
        let cid = self.calculate_cid(node)?;
        
        // Serialize the node
        let node_bytes = self.serialize_node(node)?;
        
        // Store in backend
        let storage = self.storage.lock().unwrap();
        
        // Use the non-deprecated method
        storage.put_blob(&node_bytes).await.map_err(|e| DagError::StorageError(e.to_string()))?;
        
        // Store in cache if enabled
        if !self.config.disable_cache {
            self.cache.insert(cid, Arc::new(node.clone()));
        }
        
        Ok(cid)
    }
    
    /// Get a node from the DAG
    pub async fn get_node(&self, cid: &Cid) -> Result<Option<DagNode>, DagError> {
        // Check cache first if enabled
        if !self.config.disable_cache {
            if let Some(cached_node) = self.cache.get(cid) {
                return Ok(Some(cached_node.as_ref().clone()));
            }
        }
        
        // Not in cache, check storage
        let storage = self.storage.lock().unwrap();
        
        // Use the non-deprecated method
        let node_bytes_opt = storage.get_blob(cid).await.map_err(|e| DagError::StorageError(e.to_string()))?;
        
        match node_bytes_opt {
            Some(node_bytes) => {
                // Deserialize the node
                let node = self.deserialize_node(&node_bytes)?;
                
                // Store in cache if enabled
                if !self.config.disable_cache {
                    self.cache.insert(cid.clone(), Arc::new(node.clone()));
                }
                
                Ok(Some(node))
            },
            None => Ok(None),
        }
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> cache::CacheStats {
        self.cache.stats()
    }
    
    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
    
    /// Calculate CID for a node
    fn calculate_cid(&self, node: &DagNode) -> Result<Cid, DagError> {
        // Serialize the node to calculate its hash
        let node_bytes = self.serialize_node(node)?;
        
        // Calculate hash using SHA-256
        let hash = multihash::Code::Sha2_256.digest(&node_bytes);
        
        // Create CID with raw codec (0x71)
        Ok(Cid::new_v1(0x71, hash))
    }
    
    /// Serialize a node to bytes
    fn serialize_node(&self, node: &DagNode) -> Result<Vec<u8>, DagError> {
        serde_json::to_vec(node).map_err(|e| DagError::SerializationError(e.to_string()))
    }
    
    /// Deserialize bytes to a node
    fn deserialize_node(&self, bytes: &[u8]) -> Result<DagNode, DagError> {
        serde_json::from_slice(bytes).map_err(|e| DagError::DeserializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dag_node_creation() {
        let content = b"test content".to_vec();
        let parents = vec![];
        let signer = IdentityId("did:icn:test".to_string());
        let signature = Signature(vec![1, 2, 3, 4]);
        let metadata = DagNodeMetadata::new().with_scope("test");
        
        let node = DagNode::new(content, parents, signer, signature, Some(metadata));
        assert!(node.is_ok());
        
        let node = node.unwrap();
        assert!(node.cid.is_some());
        assert_eq!(node.content, b"test content".to_vec());
        assert_eq!(node.parents.len(), 0);
        assert_eq!(node.signer.0, "did:icn:test");
        assert_eq!(node.metadata.scope, Some("test".to_string()));
    }
    
    #[test]
    fn test_lineage_attestation() {
        // Create a test DAG node first
        let content = b"test content".to_vec();
        let parents = vec![];
        let signer = IdentityId("did:icn:test".to_string());
        let signature = Signature(vec![1, 2, 3, 4]);
        let node = DagNode::new(content, parents, signer.clone(), signature.clone(), None).unwrap();
        
        // Create a fake root CID
        let mh = Code::Sha2_256.digest(b"root");
        let root_cid = Cid::new_v0(mh).unwrap();
        
        // Create a lineage attestation
        let node_cid = node.cid().unwrap();
        let proof = vec![vec![1, 2, 3]]; // Fake proof
        let attestation = LineageAttestation::new(
            root_cid,
            node_cid,
            proof,
            signer,
            signature,
            1000, // Timestamp
        );
        
        assert!(attestation.is_ok());
        let attestation = attestation.unwrap();
        assert_eq!(attestation.root_cid, root_cid);
        assert_eq!(attestation.node_cid, node_cid);
        
        // Test verification
        assert!(attestation.verify().is_ok());
    }
} 