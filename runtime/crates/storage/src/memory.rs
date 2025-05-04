use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::lock::Mutex;
use icn_models::{
    BasicStorageManager, 
    Cid, 
    DagNode, 
    DagNodeBuilder,
    DagNodeMetadata,
    DagStorageManager,
    dag_storage_codec
};

/// In-memory implementation of storage manager for DAG nodes
pub struct InMemoryStorageManager {
    /// Storage for binary blobs
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    
    /// Storage for entity-specific nodes
    nodes: Arc<Mutex<HashMap<String, HashMap<String, Vec<u8>>>>>,
    
    /// Store for DagNode objects by CID
    node_store: Arc<Mutex<HashMap<Cid, DagNode>>>,
    
    /// Store for metadata by CID
    metadata_store: Arc<Mutex<HashMap<Cid, DagNodeMetadata>>>,
}

impl InMemoryStorageManager {
    /// Create a new in-memory storage manager
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            nodes: Arc::new(Mutex::new(HashMap::new())),
            node_store: Arc::new(Mutex::new(HashMap::new())),
            metadata_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Helper function to generate a blob key from a CID
    fn blob_key(cid: &Cid) -> String {
        cid.to_string()
    }
    
    /// Helper function to generate node keys from a DID and CID
    fn node_key(did: &str, cid: &Cid) -> (String, String) {
        (did.to_string(), cid.to_string())
    }
}

impl Default for InMemoryStorageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BasicStorageManager for InMemoryStorageManager {
    async fn store_blob(&self, data: &[u8]) -> Result<Cid> {
        // Calculate the CID based on content
        let codec = dag_storage_codec();
        let serialized = codec.encode(&data)?;
        
        // Create a CID from the content
        let cid_bytes = serialized.clone();
        let mh = multihash::Sha2_256::digest(&cid_bytes);
        let cid = Cid::new_v1(0x55, mh); // 0x55 is raw binary format
        
        // Store the blob
        let key = Self::blob_key(&cid);
        let mut blobs = self.blobs.lock().await;
        blobs.insert(key, data.to_vec());
        
        Ok(cid)
    }
    
    async fn get_blob(&self, cid: &Cid) -> Result<Option<Vec<u8>>> {
        let key = Self::blob_key(cid);
        let blobs = self.blobs.lock().await;
        Ok(blobs.get(&key).cloned())
    }
    
    async fn contains_blob(&self, cid: &Cid) -> Result<bool> {
        let key = Self::blob_key(cid);
        let blobs = self.blobs.lock().await;
        Ok(blobs.contains_key(&key))
    }
    
    async fn create_namespace(&self, namespace: &str) -> Result<()> {
        let mut nodes = self.nodes.lock().await;
        if !nodes.contains_key(namespace) {
            nodes.insert(namespace.to_string(), HashMap::new());
        }
        Ok(())
    }
    
    async fn namespace_exists(&self, namespace: &str) -> Result<bool> {
        let nodes = self.nodes.lock().await;
        Ok(nodes.contains_key(namespace))
    }
    
    async fn store_in_namespace(&self, namespace: &str, key: &str, data: &[u8]) -> Result<()> {
        let mut nodes = self.nodes.lock().await;
        if let Some(ns) = nodes.get_mut(namespace) {
            ns.insert(key.to_string(), data.to_vec());
            Ok(())
        } else {
            Err(anyhow!("Namespace does not exist: {}", namespace))
        }
    }
    
    async fn get_from_namespace(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let nodes = self.nodes.lock().await;
        if let Some(ns) = nodes.get(namespace) {
            Ok(ns.get(key).cloned())
        } else {
            Ok(None)
        }
    }
    
    async fn contains_in_namespace(&self, namespace: &str, key: &str) -> Result<bool> {
        let nodes = self.nodes.lock().await;
        if let Some(ns) = nodes.get(namespace) {
            Ok(ns.contains_key(key))
        } else {
            Ok(false)
        }
    }
}

#[async_trait]
impl DagStorageManager for InMemoryStorageManager {
    async fn store_new_dag_root(
        &self,
        entity_did: &str,
        node_builder: &dyn DagNodeBuilder,
    ) -> Result<(Cid, DagNode)> {
        // Generate the node
        let node = node_builder.build()?;
        let cid = node.cid;
        
        // Store the node in the node store
        {
            let mut node_store = self.node_store.lock().await;
            node_store.insert(cid, node.clone());
        }
        
        // Store the metadata in the metadata store
        {
            let mut metadata_store = self.metadata_store.lock().await;
            metadata_store.insert(cid, node.metadata.clone());
        }
        
        // Store it using store_blob
        let serialized = dag_storage_codec().encode(&node)?;
        let _ = self.store_blob(&serialized).await?;
        
        // Store in entity's namespace
        let (did_key, node_key) = Self::node_key(entity_did, &cid);
        
        // Create entity namespace if it doesn't exist
        let mut nodes = self.nodes.lock().await;
        if !nodes.contains_key(&did_key) {
            nodes.insert(did_key.clone(), HashMap::new());
        }
        
        // Add node to entity namespace
        if let Some(entity_nodes) = nodes.get_mut(&did_key) {
            entity_nodes.insert(node_key, serialized);
        }
        
        Ok((cid, node))
    }
    
    async fn store_node(
        &self,
        entity_did: &str,
        node_builder: &dyn DagNodeBuilder,
    ) -> Result<(Cid, DagNode)> {
        // Check if namespace exists
        if !self.namespace_exists(entity_did).await? {
            return Err(anyhow!("Entity namespace does not exist: {}", entity_did));
        }
        
        // Generate the node
        let node = node_builder.build()?;
        let cid = node.cid;
        
        // Store the node in the node store
        {
            let mut node_store = self.node_store.lock().await;
            node_store.insert(cid, node.clone());
        }
        
        // Store the metadata in the metadata store
        {
            let mut metadata_store = self.metadata_store.lock().await;
            metadata_store.insert(cid, node.metadata.clone());
        }
        
        // Serialize the node
        let serialized = dag_storage_codec().encode(&node)?;
        
        // Store the raw blob
        let _ = self.store_blob(&serialized).await?;
        
        // Get entity namespace keys
        let (did_key, node_key) = Self::node_key(entity_did, &cid);
        
        // Store in entity namespace
        let mut nodes = self.nodes.lock().await;
        if let Some(entity_nodes) = nodes.get_mut(&did_key) {
            entity_nodes.insert(node_key, serialized);
        } else {
            return Err(anyhow!("Entity namespace does not exist: {}", entity_did));
        }
        
        Ok((cid, node))
    }
    
    async fn get_node(&self, entity_did: &str, cid: &Cid) -> Result<Option<DagNode>> {
        // Try to get directly from node store first
        {
            let node_store = self.node_store.lock().await;
            if let Some(node) = node_store.get(cid) {
                return Ok(Some(node.clone()));
            }
        }
        
        // Otherwise, get from serialized storage
        if let Some(bytes) = self.get_node_bytes(entity_did, cid).await? {
            // Deserialize
            let node = dag_storage_codec().decode::<DagNode>(&bytes)?;
            
            // Cache it for future use
            {
                let mut node_store = self.node_store.lock().await;
                node_store.insert(*cid, node.clone());
            }
            
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }
    
    async fn contains_node(&self, entity_did: &str, cid: &Cid) -> Result<bool> {
        // Check if in node store first
        {
            let node_store = self.node_store.lock().await;
            if node_store.contains_key(cid) {
                return Ok(true);
            }
        }
        
        // Otherwise check in entity namespace
        let (did_key, node_key) = Self::node_key(entity_did, cid);
        let nodes = self.nodes.lock().await;
        
        if let Some(entity_nodes) = nodes.get(&did_key) {
            Ok(entity_nodes.contains_key(&node_key))
        } else {
            Ok(false)
        }
    }
    
    async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> Result<Option<Vec<u8>>> {
        let (did_key, node_key) = Self::node_key(entity_did, cid);
        let nodes = self.nodes.lock().await;
        
        if let Some(entity_nodes) = nodes.get(&did_key) {
            Ok(entity_nodes.get(&node_key).cloned())
        } else {
            Ok(None)
        }
    }
    
    async fn store_nodes_batch(
        &self,
        entity_did: &str,
        node_builders: Vec<&dyn DagNodeBuilder>,
    ) -> Result<Vec<(Cid, DagNode)>> {
        // Check if namespace exists
        if !self.namespace_exists(entity_did).await? {
            return Err(anyhow!("Entity namespace does not exist: {}", entity_did));
        }
        
        let mut results = Vec::new();
        
        for builder in node_builders {
            let (cid, node) = self.store_node(entity_did, builder).await?;
            results.push((cid, node));
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityId;
    use libipld::Ipld;
    
    struct TestDagNodeBuilder {
        issuer: String,
        parents: Vec<Cid>,
        metadata: DagNodeMetadata,
        payload: Ipld,
    }
    
    impl TestDagNodeBuilder {
        fn new() -> Self {
            Self {
                issuer: "did:icn:test".to_string(),
                parents: Vec::new(),
                metadata: DagNodeMetadata {
                    timestamp: 1234567890,
                    sequence: 1,
                    content_type: Some("application/json".to_string()),
                    tags: vec!["test".to_string()],
                },
                payload: Ipld::Null,
            }
        }
    }
    
    impl DagNodeBuilder for TestDagNodeBuilder {
        fn with_issuer(mut self, issuer: String) -> Self {
            self.issuer = issuer;
            self
        }
        
        fn with_parents(mut self, parents: Vec<Cid>) -> Self {
            self.parents = parents;
            self
        }
        
        fn with_metadata(mut self, metadata: DagNodeMetadata) -> Self {
            self.metadata = metadata;
            self
        }
        
        fn with_payload(mut self, payload: Ipld) -> Self {
            self.payload = payload;
            self
        }
        
        fn build(self) -> Result<DagNode> {
            // Create a dummy CID for demonstration
            let mh = multihash::Sha2_256::digest(&[0, 1, 2, 3]);
            let cid = Cid::new_v1(0x55, mh);
            
            Ok(DagNode {
                cid,
                parents: self.parents,
                issuer: IdentityId::new(self.issuer),
                signature: vec![1, 2, 3, 4],
                payload: self.payload,
                metadata: self.metadata,
            })
        }
        
        fn new() -> Self {
            Self::new()
        }
    }
    
    #[tokio::test]
    async fn test_basic_storage_manager() -> Result<()> {
        let storage = InMemoryStorageManager::new();
        
        // Test blob storage
        let data = b"hello world";
        let cid = storage.store_blob(data).await?;
        let retrieved = storage.get_blob(&cid).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
        
        // Test namespace operations
        storage.create_namespace("test_namespace").await?;
        assert!(storage.namespace_exists("test_namespace").await?);
        
        storage.store_in_namespace("test_namespace", "key1", b"value1").await?;
        let value = storage.get_from_namespace("test_namespace", "key1").await?;
        assert!(value.is_some());
        assert_eq!(value.unwrap(), b"value1");
        
        assert!(storage.contains_in_namespace("test_namespace", "key1").await?);
        assert!(!storage.contains_in_namespace("test_namespace", "key2").await?);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_dag_storage_manager() -> Result<()> {
        let storage = InMemoryStorageManager::new();
        let entity_did = "did:icn:test_entity";
        
        // Create and store a root node
        let builder = TestDagNodeBuilder::new()
            .with_issuer(entity_did.to_string())
            .with_payload(Ipld::String("root node".to_string()));
            
        let (root_cid, root_node) = storage.store_new_dag_root(entity_did, &builder).await?;
        
        // Test retrieval
        let retrieved_node = storage.get_node(entity_did, &root_cid).await?;
        assert!(retrieved_node.is_some());
        let retrieved = retrieved_node.unwrap();
        assert_eq!(retrieved.cid, root_node.cid);
        
        // Test contains
        assert!(storage.contains_node(entity_did, &root_cid).await?);
        
        // Create and store a child node
        let child_builder = TestDagNodeBuilder::new()
            .with_issuer(entity_did.to_string())
            .with_parents(vec![root_cid])
            .with_payload(Ipld::String("child node".to_string()));
            
        let (child_cid, child_node) = storage.store_node(entity_did, &child_builder).await?;
        
        // Test child node retrieval
        let retrieved_child = storage.get_node(entity_did, &child_cid).await?;
        assert!(retrieved_child.is_some());
        assert_eq!(retrieved_child.unwrap().parents, vec![root_cid]);
        
        // Test batch storage
        let batch_builders = vec![
            &TestDagNodeBuilder::new()
                .with_issuer(entity_did.to_string())
                .with_parents(vec![child_cid])
                .with_payload(Ipld::String("batch node 1".to_string())) as &dyn DagNodeBuilder,
            &TestDagNodeBuilder::new()
                .with_issuer(entity_did.to_string())
                .with_parents(vec![child_cid])
                .with_payload(Ipld::String("batch node 2".to_string())) as &dyn DagNodeBuilder,
        ];
        
        let batch_results = storage.store_nodes_batch(entity_did, batch_builders).await?;
        assert_eq!(batch_results.len(), 2);
        
        for (cid, _) in batch_results {
            assert!(storage.contains_node(entity_did, &cid).await?);
        }
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_node_store_cache() -> Result<()> {
        let storage = InMemoryStorageManager::new();
        let entity_did = "did:icn:test_entity";
        
        // Create and store a node
        let builder = TestDagNodeBuilder::new()
            .with_issuer(entity_did.to_string())
            .with_payload(Ipld::String("test node".to_string()));
            
        let (cid, original_node) = storage.store_new_dag_root(entity_did, &builder).await?;
        
        // Get node directly from node_store
        {
            let node_store = storage.node_store.lock().await;
            assert!(node_store.contains_key(&cid));
            
            let cached_node = node_store.get(&cid).unwrap();
            assert_eq!(cached_node.cid, original_node.cid);
        }
        
        // Get metadata from metadata_store
        {
            let metadata_store = storage.metadata_store.lock().await;
            assert!(metadata_store.contains_key(&cid));
            
            let cached_metadata = metadata_store.get(&cid).unwrap();
            assert_eq!(cached_metadata.sequence, original_node.metadata.sequence);
        }
        
        Ok(())
    }
} 