use crate::{
    Cid,
    DagNode,
    DagNodeBuilder,
    DagNodeMetadata,
    StorageBackend,
    StorageError,
    StorageResult,
    BasicStorageManager,
    DagStorageManager,
    dag_storage_codec,
};
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use libipld::Ipld;
use icn_identity::IdentityId;

// Stub implementations for testing the interfaces

#[derive(Clone)]
struct TestDagNodeBuilder {
    issuer: Option<String>,
    parents: Vec<Cid>,
    metadata: Option<DagNodeMetadata>,
    payload: Option<Ipld>,
}

impl DagNodeBuilder for TestDagNodeBuilder {
    fn with_issuer(mut self, issuer: String) -> Self {
        self.issuer = Some(issuer);
        self
    }
    
    fn with_parents(mut self, parents: Vec<Cid>) -> Self {
        self.parents = parents;
        self
    }
    
    fn with_metadata(mut self, metadata: DagNodeMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    fn with_payload(mut self, payload: Ipld) -> Self {
        self.payload = Some(payload);
        self
    }
    
    fn build(self) -> crate::Result<DagNode> {
        // Create a dummy CID
        let cid = Cid::default();
        
        // Build the node with the fields provided
        Ok(DagNode {
            cid,
            parents: self.parents,
            issuer: IdentityId::new(self.issuer.unwrap_or_else(|| "unknown".to_string())),
            signature: vec![],
            payload: self.payload.unwrap_or(Ipld::Null),
            metadata: self.metadata.unwrap_or_else(|| DagNodeMetadata {
                timestamp: 0,
                sequence: 0,
                content_type: None,
                tags: vec![],
            }),
        })
    }
    
    fn new() -> Self {
        Self {
            issuer: None,
            parents: vec![],
            metadata: None,
            payload: None,
        }
    }
}

struct TestStorageBackend {
    data: HashMap<String, Vec<u8>>,
}

impl TestStorageBackend {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
    
    fn cid_to_key(&self, cid: &Cid) -> String {
        cid.to_string()
    }
}

#[async_trait]
impl StorageBackend for TestStorageBackend {
    // Minimal implementation for testing
    async fn put_blob(&self, _value_bytes: &[u8]) -> StorageResult<Cid> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn get_blob(&self, _content_cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn contains_blob(&self, _content_cid: &Cid) -> StorageResult<bool> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn delete_blob(&self, _content_cid: &Cid) -> StorageResult<()> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn put_kv(&self, _key_cid: Cid, _value_bytes: Vec<u8>) -> StorageResult<()> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn get_kv(&self, _key_cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn contains_kv(&self, _key_cid: &Cid) -> StorageResult<bool> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn delete_kv(&self, _key_cid: &Cid) -> StorageResult<()> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn begin_transaction(&self) -> StorageResult<()> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn commit_transaction(&self) -> StorageResult<()> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
    
    async fn rollback_transaction(&self) -> StorageResult<()> {
        Err(StorageError::NotSupported("Not implemented for test".to_string()))
    }
}

// Unit tests to validate interfaces

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dag_node_builder() {
        let builder = TestDagNodeBuilder::new()
            .with_issuer("did:icn:test".to_string())
            .with_metadata(DagNodeMetadata {
                timestamp: 1234567890,
                sequence: 1,
                content_type: Some("application/json".to_string()),
                tags: vec!["test".to_string()],
            })
            .with_payload(Ipld::String("Hello, world!".to_string()));
            
        let node = builder.build().unwrap();
        assert_eq!(node.issuer.to_string(), "did:icn:test");
        assert_eq!(node.metadata.sequence, 1);
        assert_eq!(node.metadata.content_type, Some("application/json".to_string()));
        
        match node.payload {
            Ipld::String(s) => assert_eq!(s, "Hello, world!"),
            _ => panic!("Expected String payload"),
        }
    }
    
    #[test]
    fn test_dag_codec() {
        let codec = dag_storage_codec();
        
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestStruct {
            name: String,
            value: i32,
        }
        
        let test_data = TestStruct {
            name: "test".to_string(),
            value: 42,
        };
        
        let encoded = codec.encode(&test_data).unwrap();
        let decoded: TestStruct = codec.decode(&encoded).unwrap();
        
        assert_eq!(decoded, test_data);
    }
} 