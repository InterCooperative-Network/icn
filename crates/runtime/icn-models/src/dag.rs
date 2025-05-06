/*!
 * DAG (Directed Acyclic Graph) data models
 *
 * This module defines the core DAG data structures used by the ICN Runtime.
 */

use crate::Cid;
use icn_identity::IdentityId;
use libipld::Ipld;
use serde::{Deserialize, Serialize};

/// Metadata for a DAG node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNodeMetadata {
    /// Unix timestamp for this node
    pub timestamp: u64,
    
    /// Sequence number within the DAG
    pub sequence: u64,
    
    /// Content type/format
    pub content_type: Option<String>,
    
    /// Additional tags
    pub tags: Vec<String>,
}

/// Interface for DAG node construction
pub trait DagNodeBuilder {
    /// Set the issuer of this node
    fn with_issuer(self, issuer: String) -> Self;
    
    /// Set the parent nodes of this node
    fn with_parents(self, parents: Vec<Cid>) -> Self;
    
    /// Set the metadata for this node
    fn with_metadata(self, metadata: DagNodeMetadata) -> Self;
    
    /// Set the payload for this node
    fn with_payload(self, payload: Ipld) -> Self;
    
    /// Build the DAG node
    fn build(self) -> crate::Result<DagNode>;
    
    /// Create a new empty builder
    fn new() -> Self;
}

/// A node in a Directed Acyclic Graph (DAG)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// The Content Identifier (CID) for this node
    pub cid: Cid,
    
    /// Parent node CIDs
    pub parents: Vec<Cid>,
    
    /// Issuer of this node
    pub issuer: IdentityId,
    
    /// Signature bytes
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    
    /// Payload data
    pub payload: Ipld,
    
    /// Metadata for this node
    pub metadata: DagNodeMetadata,
}

/// Types of DAG networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DagType {
    /// A cooperative's DAG
    Cooperative,
    
    /// A community's DAG
    Community,
    
    /// A personal DAG
    Personal,
    
    /// A project's DAG
    Project,
    
    /// Federation-level DAG
    Federation,
}

/// Interface for DAG codec operations
pub trait DagCodec {
    /// Encode a DAG node to bytes
    fn encode<T: Serialize>(&self, node: &T) -> crate::Result<Vec<u8>>;
    
    /// Decode bytes to a DAG node
    fn decode<T: for<'de> Deserialize<'de>>(&self, bytes: &[u8]) -> crate::Result<T>;
}

/// Default implementation of DagCodec using CBOR
pub struct DagCborCodec;

impl DagCodec for DagCborCodec {
    fn encode<T: Serialize>(&self, node: &T) -> crate::Result<Vec<u8>> {
        let bytes = serde_ipld_dagcbor::to_vec(node)
            .map_err(|e| crate::ModelError::SerializationError(e.to_string()))?;
        Ok(bytes)
    }
    
    fn decode<T: for<'de> Deserialize<'de>>(&self, bytes: &[u8]) -> crate::Result<T> {
        let value = serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|e| crate::ModelError::DeserializationError(e.to_string()))?;
        Ok(value)
    }
}

/// Get a default codec implementation for DAG storage
pub fn dag_storage_codec() -> impl DagCodec {
    DagCborCodec
} 