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
use libipld::{Ipld, ipld, error::Error as IpldError};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use std::sync::{Arc, Mutex};
use icn_models::storage::StorageBackend;
use anyhow::Result;

pub mod audit;
pub mod cache;
pub mod query;
pub mod events;

/// Helper function to create a multihash using SHA-256
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

/// Errors that can occur in DAG operations
#[derive(Debug, Error)]
pub enum DagError {
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    
    #[error("Invalid node: {0}")]
    InvalidNode(String),
    
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    
    #[error("Content error: {0}")]
    ContentError(String),
    
    #[error("Codec error: {0}")]
    CodecError(#[from] IpldError),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

/// Result type for DAG operations
pub type DagResult<T> = std::result::Result<T, DagError>;

/// Metadata for a DAG node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagNodeMetadata {
    /// UNIX timestamp in seconds
    pub timestamp: u64,
    
    /// Sequence number for ordering
    pub sequence: u64,
    
    /// Content type/format
    pub content_type: Option<String>,
    
    /// Additional tags
    pub tags: Vec<String>,
}

impl DagNodeMetadata {
    /// Create new metadata with current timestamp
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            sequence: 0,
            content_type: None,
            tags: Vec::new(),
        }
    }
    
    /// Create new metadata with specific timestamp
    pub fn with_timestamp(timestamp: u64) -> Self {
        Self {
            timestamp,
            sequence: 0,
            content_type: None,
            tags: Vec::new(),
        }
    }
    
    /// Set sequence number
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }
    
    /// Set content type
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
    
    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

impl Default for DagNodeMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A node in the DAG
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagNode {
    /// IPLD payload data
    pub payload: Ipld,
    
    /// Parent CIDs
    pub parents: Vec<Cid>,
    
    /// Identity of the issuer
    pub issuer: IdentityId,
    
    /// Signature over the node content
    pub signature: Vec<u8>,
    
    /// Metadata
    pub metadata: DagNodeMetadata,
}

/// Builder for creating DAG nodes
pub struct DagNodeBuilder {
    payload: Option<Ipld>,
    parents: Vec<Cid>,
    issuer: Option<IdentityId>,
    metadata: DagNodeMetadata,
}

impl DagNodeBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            payload: None,
            parents: Vec::new(),
            issuer: None,
            metadata: DagNodeMetadata::new(),
        }
    }
    
    /// Set the payload
    pub fn payload(mut self, payload: Ipld) -> Self {
        self.payload = Some(payload);
        self
    }
    
    /// Set the parents
    pub fn parents(mut self, parents: Vec<Cid>) -> Self {
        self.parents = parents;
        self
    }
    
    /// Add a parent
    pub fn parent(mut self, parent: Cid) -> Self {
        self.parents.push(parent);
        self
    }
    
    /// Set the issuer
    pub fn issuer(mut self, issuer: impl Into<IdentityId>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }
    
    /// Set the metadata
    pub fn metadata(mut self, metadata: DagNodeMetadata) -> Self {
        self.metadata = metadata;
        self
    }
    
    /// Set the timestamp
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.metadata.timestamp = timestamp;
        self
    }
    
    /// Set the sequence
    pub fn sequence(mut self, sequence: u64) -> Self {
        self.metadata.sequence = sequence;
        self
    }
    
    /// Set content type
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.metadata.content_type = Some(content_type.into());
        self
    }
    
    /// Add a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.metadata.tags.push(tag.into());
        self
    }
    
    /// Build the node (without signing)
    pub fn build(self) -> Result<DagNode> {
        let payload = self.payload
            .ok_or_else(|| anyhow::anyhow!("Payload is required"))?;
            
        let issuer = self.issuer
            .ok_or_else(|| anyhow::anyhow!("Issuer is required"))?;
        
        // In a real implementation, this would sign the node content
        // For now, using a placeholder signature
        let signature = vec![1, 2, 3, 4];
        
        Ok(DagNode {
            payload,
            parents: self.parents,
            issuer,
            signature,
            metadata: self.metadata,
        })
    }
    
    /// Build with automatic signing
    pub fn build_signed(self, signer: &impl Signer) -> Result<DagNode> {
        let payload = self.payload
            .ok_or_else(|| anyhow::anyhow!("Payload is required"))?;
            
        let issuer = self.issuer
            .ok_or_else(|| anyhow::anyhow!("Issuer is required"))?;
        
        // Create the unsigned node
        let unsigned = DagNode {
            payload: payload.clone(),
            parents: self.parents.clone(),
            issuer: issuer.clone(),
            signature: Vec::new(), // Empty signature for now
            metadata: self.metadata.clone(),
        };
        
        // Sign the node
        let signature = signer.sign(&unsigned)?;
        
        Ok(DagNode {
            payload,
            parents: self.parents,
            issuer,
            signature,
            metadata: self.metadata,
        })
    }
}

/// Trait for signing nodes
pub trait Signer: Send + Sync {
    /// Sign a DAG node
    fn sign(&self, node: &DagNode) -> Result<Vec<u8>>;
    
    /// Verify a node's signature
    fn verify(&self, node: &DagNode) -> Result<bool>;
}

/// DAG manager interface
#[async_trait::async_trait]
pub trait DagManager: Send + Sync {
    /// Store a new DAG node
    async fn store_node(&self, node: &DagNode) -> Result<Cid>;
    
    /// Store multiple DAG nodes in a batch
    async fn store_nodes_batch(&self, nodes: Vec<DagNode>) -> Result<Vec<Cid>> {
        let mut cids = Vec::with_capacity(nodes.len());
        
        for node in nodes {
            let cid = self.store_node(&node).await?;
            cids.push(cid);
        }
        
        Ok(cids)
    }
    
    /// Retrieve a DAG node by CID
    async fn get_node(&self, cid: &Cid) -> Result<Option<DagNode>>;
    
    /// Check if a node exists
    async fn contains_node(&self, cid: &Cid) -> Result<bool>;
    
    /// Get parents of a node
    async fn get_parents(&self, cid: &Cid) -> Result<Vec<DagNode>>;
    
    /// Get children of a node
    async fn get_children(&self, cid: &Cid) -> Result<Vec<DagNode>>;
    
    /// Verify a node's signature
    async fn verify_node(&self, cid: &Cid) -> Result<bool>;
    
    /// Get the latest nodes in the DAG (tips)
    async fn get_tips(&self) -> Result<Vec<Cid>>;
}

pub use events::DagEvent;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dag_node_builder() {
        let builder = DagNodeBuilder::new()
            .payload(ipld!({ "key": "value" }))
            .parent(Cid::new_v1(0x71, create_sha256_multihash(b"parent")))
            .issuer(IdentityId("did:icn:test".to_string()))
            .timestamp(123456789)
            .tag("test-tag");
            
        let node = builder.build().unwrap();
        
        assert_eq!(node.issuer.0, "did:icn:test");
        assert_eq!(node.parents.len(), 1);
        assert_eq!(node.metadata.timestamp, 123456789);
        assert_eq!(node.metadata.tags, vec!["test-tag"]);
    }
} 