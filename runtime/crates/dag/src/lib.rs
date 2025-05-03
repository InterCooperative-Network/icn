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
use icn_storage::StorageBackend;
use anyhow::Result;

pub mod audit;

/// Helper function to create a multihash using SHA-256
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

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
    
    #[error("Codec error: {0}")]
    CodecError(#[from] IpldError),
    
    #[error("Content error: {0}")]
    ContentError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Builder error: Missing required field '{0}'")]
    BuilderMissingField(String),
}

/// Result type for DAG operations
pub type DagResult<T> = std::result::Result<T, DagError>;

/// Metadata for a DAG node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagNodeMetadata {
    /// Timestamp of when this node was created (unix timestamp in seconds)
    #[serde(with = "serde_bytes")]
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

/// Represents a node in the DAG, compatible with IPLD and DagCbor encoding.
/// The CID is *not* stored within the node itself; it's derived from the encoded bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagNode {
    /// Arbitrary IPLD data payload of this node.
    pub payload: Ipld,
    
    /// Parent CIDs (links) of this node.
    pub parents: Vec<Cid>,
    
    /// Identity (DID) that issued/signed this node.
    pub issuer: IdentityId,
    
    /// Signature over the canonicalized representation of the node (excluding signature field itself).
    /// The exact signing process needs definition (e.g., encode, hash, sign hash).
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    
    /// Metadata associated with this node.
    pub metadata: DagNodeMetadata,
}

impl DagNode {
    /// Verify the signature of this node (Placeholder).
    /// Note: Verification requires obtaining the canonical bytes used for signing,
    /// which depends on the DagCborCodec implementation details.
    pub fn verify_signature(&self, _public_key_jwk: &ssi::jwk::JWK) -> DagResult<()> {
        if self.signature.is_empty() {
            return Err(DagError::SignatureVerificationFailed);
        }
        Ok(())
    }
    
    /// Get the links (parent CIDs) of this node.
    pub fn links(&self) -> &[Cid] {
        &self.parents
    }
    
    /// Get the timestamp of this node.
    pub fn timestamp(&self) -> u64 {
        self.metadata.timestamp
    }
}

/// Builder for creating DagNode instances.
#[derive(Default)]
pub struct DagNodeBuilder {
    payload: Option<Ipld>,
    parents: Option<Vec<Cid>>,
    issuer: Option<IdentityId>,
    signature: Option<Vec<u8>>,
    metadata: Option<DagNodeMetadata>,
}

impl DagNodeBuilder {
    pub fn new() -> Self {
        Default::default()
    }
    
    pub fn payload(mut self, payload: Ipld) -> Self {
        self.payload = Some(payload);
        self
    }
    
    pub fn parents(mut self, parents: Vec<Cid>) -> Self {
        self.parents = Some(parents);
        self
    }
    
    pub fn issuer(mut self, issuer: IdentityId) -> Self {
        self.issuer = Some(issuer);
        self
    }
    
    pub fn signature(mut self, signature: Vec<u8>) -> Self {
        self.signature = Some(signature);
        self
    }
    
    pub fn metadata(mut self, metadata: DagNodeMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    pub fn build(self) -> DagResult<DagNode> {
        let issuer = self.issuer.ok_or_else(|| DagError::BuilderMissingField("issuer".to_string()))?;
        let payload = self.payload.ok_or_else(|| DagError::BuilderMissingField("payload".to_string()))?;
        let signature = self.signature.ok_or_else(|| DagError::BuilderMissingField("signature".to_string()))?;
        
        Ok(DagNode {
            payload,
            parents: self.parents.unwrap_or_default(),
            issuer,
            signature,
            metadata: self.metadata.unwrap_or_default(),
        })
    }
}

/// Calculates a Merkle root for a set of CIDs.
/// Updated to take CIDs directly instead of DagNodes.
pub fn calculate_merkle_root(cids: &[Cid]) -> DagResult<Cid> {
    if cids.is_empty() {
        return Err(DagError::InvalidNode("Empty CID list".to_string()));
    }
    
    // Extract bytes of CIDs
    let cid_bytes_list: Vec<Vec<u8>> = cids.iter().map(|cid| cid.to_bytes()).collect();
    
    // Simplified merkle root calculation - just hash all CID bytes together
    // In a real implementation, this would use a proper Merkle tree (like merkle-cbt?)
    let mut combined = Vec::new();
    for cid_bytes in cid_bytes_list {
        combined.extend_from_slice(&cid_bytes);
    }
    
    let mh = cid::multihash::Multihash::wrap(
        cid::multihash::Code::Sha2_256.into(),
        &Sha256::digest(&combined)
    ).map_err(|e| DagError::InvalidCid(format!("Failed to wrap Merkle root hash: {}", e)))?;
    
    // Use DagCbor codec for the root CID? Or Raw? Using Raw (0x55) for now.
    let root_cid = Cid::new_v1(0x55, mh);
    
    Ok(root_cid)
}

/// Represents a lineage attestation for a DAG node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageAttestation {
    /// Root CID of the DAG
    pub root_cid: Cid,
    
    /// CID of the attested node
    pub node_cid: Cid,
    
    /// Merkle proof of inclusion
    #[serde(with = "serde_bytes")]
    pub proof: Vec<Vec<u8>>,
    
    /// Identity that signed this attestation
    pub signer: IdentityId,
    
    /// Signature of this attestation
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    
    /// Timestamp of when this attestation was created
    #[serde(with = "serde_bytes")]
    pub timestamp: u64,
}

impl LineageAttestation {
    /// Create a new lineage attestation
    pub fn new(
        root_cid: Cid,
        node_cid: Cid,
        proof: Vec<Vec<u8>>,
        signer: IdentityId,
        signature: Vec<u8>,
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
        if self.signature.is_empty() {
            return Err(DagError::SignatureVerificationFailed);
        }
        
        if self.proof.is_empty() {
            return Err(DagError::MerkleVerificationFailed("Empty proof".to_string()));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libipld::ipld;
    use crate::codec::DagCborCodec;
    use libipld::codec::Codec;
    
    #[test]
    fn test_dag_node_builder_and_structure() {
        let issuer_did = IdentityId::new("did:example:issuer");
        let parent_cid = Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
        let payload_data = ipld!({ "message": "hello world", "value": 123 });
        let signature_bytes = vec![1, 2, 3, 4, 5];
        
        let builder = DagNodeBuilder::new()
            .issuer(issuer_did.clone())
            .payload(payload_data.clone())
            .parents(vec![parent_cid])
            .signature(signature_bytes.clone())
            .metadata(DagNodeMetadata::new().with_sequence(1));
        
        let node = builder.build().unwrap();
        
        assert_eq!(node.issuer, issuer_did);
        assert_eq!(node.payload, payload_data);
        assert_eq!(node.parents, vec![parent_cid]);
        assert_eq!(node.signature, signature_bytes);
        assert!(node.metadata.sequence.is_some());
        assert_eq!(node.metadata.sequence.unwrap(), 1);
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert!(now >= node.metadata.timestamp && now - node.metadata.timestamp < 5);
    }
    
    #[test]
    fn test_dag_node_builder_missing_fields() {
        let builder_no_issuer = DagNodeBuilder::new().payload(ipld!(null));
        let result_no_issuer = builder_no_issuer.build();
        assert!(matches!(result_no_issuer, Err(DagError::BuilderMissingField(field)) if field == "issuer"));
        
        let builder_no_payload = DagNodeBuilder::new().issuer(IdentityId::new("did:ex:1"));
        let result_no_payload = builder_no_payload.build();
        assert!(matches!(result_no_payload, Err(DagError::BuilderMissingField(field)) if field == "payload"));
        
        let builder_no_sig = DagNodeBuilder::new()
            .issuer(IdentityId::new("did:ex:1"))
            .payload(ipld!(true));
        let result_no_sig = builder_no_sig.build();
        assert!(matches!(result_no_sig, Err(DagError::BuilderMissingField(field)) if field == "signature"));
    }
    
    #[test]
    fn test_dag_node_cbor_encoding() {
        let node = DagNodeBuilder::new()
            .issuer(IdentityId::new("did:example:issuer"))
            .payload(ipld!({ "data": [1, 2, 3] }))
            .parents(vec![])
            .signature(vec![10, 20, 30])
            .metadata(DagNodeMetadata::new().with_sequence(0))
            .build()
            .unwrap();
        
        let codec = DagCborCodec;
        let encoded_bytes = codec.encode(&node);
        assert!(encoded_bytes.is_ok());
        let bytes = encoded_bytes.unwrap();
        
        assert!(bytes[0] >= 0xa0 && bytes[0] <= 0xbf);
        
        let decoded_node: std::result::Result<DagNode, _> = codec.decode(&bytes);
        assert!(decoded_node.is_ok());
        assert_eq!(node, decoded_node.unwrap());
    }
    
    #[test]
    fn test_updated_calculate_merkle_root() {
        let cid1 = Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
        let cid2 = Cid::try_from("bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyke").unwrap();
        
        let cids = vec![cid1, cid2];
        let root_result = calculate_merkle_root(&cids);
        assert!(root_result.is_ok());
        let root_cid = root_result.unwrap();
        
        assert_eq!(root_cid.version(), cid::Version::V1);
        assert_eq!(root_cid.codec(), 0x55);
        assert_eq!(root_cid.hash().code(), u64::from(cid::multihash::Code::Sha2_256));
        
        let empty_cids: Vec<Cid> = vec![];
        let empty_root_result = calculate_merkle_root(&empty_cids);
        assert!(matches!(empty_root_result, Err(DagError::InvalidNode(_))));
    }
} 