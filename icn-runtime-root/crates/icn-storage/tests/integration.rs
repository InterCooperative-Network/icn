use anyhow::Result;
use icn_models::{
    BasicStorageManager,
    Cid,
    DagNode,
    DagNodeBuilder,
    DagNodeMetadata,
    DagStorageManager,
    dag_storage_codec,
};
use icn_identity::IdentityId;
use icn_storage::InMemoryStorageManager;
use libipld::{multihash, Ipld};
use rand::{thread_rng, Rng};

// Test DagNodeBuilder helper for creating test nodes
struct TestNodeBuilder {
    issuer: String,
    parents: Vec<Cid>,
    metadata: DagNodeMetadata,
    payload: Ipld,
}

impl TestNodeBuilder {
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

    fn with_random_content() -> Self {
        // Generate random test data
        let mut rng = thread_rng();
        let random_number: u64 = rng.gen();
        
        Self {
            issuer: "did:icn:test".to_string(),
            parents: Vec::new(),
            metadata: DagNodeMetadata {
                timestamp: 1234567890,
                sequence: random_number,
                content_type: Some("application/json".to_string()),
                tags: vec!["test".to_string(), format!("random-{}", random_number)],
            },
            payload: Ipld::String(format!("random-content-{}", random_number)),
        }
    }
}

impl DagNodeBuilder for TestNodeBuilder {
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
        // Create a deterministic CID for reproducible tests
        let content = format!("{}-{:?}-{:?}", 
            self.issuer, 
            self.metadata.sequence,
            self.payload
        );
        let mh = multihash::Sha2_256::digest(content.as_bytes());
        let cid = Cid::new_v1(0x55, mh);
        
        Ok(DagNode {
            cid,
            parents: self.parents,
            issuer: IdentityId::new(self.issuer),
            signature: vec![1, 2, 3, 4], // Test signature
            payload: self.payload,
            metadata: self.metadata,
        })
    }
    
    fn new() -> Self {
        Self::new()
    }
}

// Helper function to create a random CID for testing
fn create_random_cid() -> Cid {
    let mut rng = thread_rng();
    let random_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    let mh = multihash::Sha2_256::digest(&random_bytes);
    Cid::new_v1(0x55, mh)
}

#[tokio::test]
async fn test_store_and_retrieve_metadata() -> Result<()> {
    // Initialize storage manager
    let storage = InMemoryStorageManager::new();
    
    // Create a random CID for testing
    let cid = create_random_cid();
    
    // Create metadata object with unique values
    let metadata = DagNodeMetadata {
        timestamp: 1618240956,
        sequence: 42,
        content_type: Some("application/json".to_string()),
        tags: vec!["integration-test".to_string(), "metadata".to_string()],
    };
    
    // Store metadata in memory storage
    {
        let mut metadata_store = storage.metadata_store.lock().await;
        metadata_store.insert(cid, metadata.clone());
    }
    
    // Verify metadata retrieval
    {
        let metadata_store = storage.metadata_store.lock().await;
        let retrieved = metadata_store.get(&cid);
        
        // Assert metadata exists and field values match
        assert!(retrieved.is_some(), "Failed to retrieve metadata");
        let retrieved_metadata = retrieved.unwrap();
        assert_eq!(retrieved_metadata.timestamp, metadata.timestamp);
        assert_eq!(retrieved_metadata.sequence, metadata.sequence);
        assert_eq!(retrieved_metadata.content_type, metadata.content_type);
        assert_eq!(retrieved_metadata.tags, metadata.tags);
    }
    
    // Test contains_metadata (indirectly through metadata_store access)
    {
        let metadata_store = storage.metadata_store.lock().await;
        assert!(metadata_store.contains_key(&cid), "Metadata should exist in storage");
        
        // Test with a non-existent CID
        let random_cid = create_random_cid();
        assert!(!metadata_store.contains_key(&random_cid), "Non-existent metadata should not exist");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_store_and_retrieve_node() -> Result<()> {
    // Initialize storage manager
    let storage = InMemoryStorageManager::new();
    let entity_did = "did:icn:test_entity";
    
    // Create a test node with associated metadata
    let node_builder = TestNodeBuilder::with_random_content()
        .with_issuer(entity_did.to_string());
    
    // Store the node (this will create the entity namespace)
    let (cid, original_node) = storage.store_new_dag_root(entity_did, &node_builder).await?;
    
    // Retrieve the node
    let retrieved_opt = storage.get_node(entity_did, &cid).await?;
    
    // Assert node exists and validate its structure
    assert!(retrieved_opt.is_some(), "Failed to retrieve node");
    let retrieved_node = retrieved_opt.unwrap();
    
    // Validate CID
    assert_eq!(retrieved_node.cid, original_node.cid, "CIDs should match");
    
    // Validate other fields
    assert_eq!(retrieved_node.issuer, original_node.issuer, "Issuers should match");
    assert_eq!(retrieved_node.parents, original_node.parents, "Parents should match");
    assert_eq!(retrieved_node.payload, original_node.payload, "Payloads should match");
    
    // Validate metadata fields
    assert_eq!(retrieved_node.metadata.timestamp, original_node.metadata.timestamp, "Timestamps should match");
    assert_eq!(retrieved_node.metadata.sequence, original_node.metadata.sequence, "Sequences should match");
    assert_eq!(retrieved_node.metadata.content_type, original_node.metadata.content_type, "Content types should match");
    assert_eq!(retrieved_node.metadata.tags, original_node.metadata.tags, "Tags should match");
    
    // Test contains_node
    let contains = storage.contains_node(entity_did, &cid).await?;
    assert!(contains, "Node should exist in storage");
    
    Ok(())
}

#[tokio::test]
async fn test_nonexistent_entries() -> Result<()> {
    // Initialize storage manager
    let storage = InMemoryStorageManager::new();
    let entity_did = "did:icn:test_entity";
    
    // Create a namespace for the entity
    storage.create_namespace(entity_did).await?;
    
    // Generate a random CID for a node that doesn't exist
    let unknown_cid = create_random_cid();
    
    // Attempt to retrieve a non-existent node
    let node_result = storage.get_node(entity_did, &unknown_cid).await?;
    assert!(node_result.is_none(), "Node should not exist");
    
    // Verify contains_node returns false
    let contains_node = storage.contains_node(entity_did, &unknown_cid).await?;
    assert!(!contains_node, "Contains should return false for non-existent node");
    
    // Attempt to retrieve non-existent metadata
    let metadata_store = storage.metadata_store.lock().await;
    let metadata_result = metadata_store.get(&unknown_cid);
    assert!(metadata_result.is_none(), "Metadata should not exist");
    
    // Verify contains_key returns false in metadata store
    assert!(!metadata_store.contains_key(&unknown_cid), "Contains should return false for non-existent metadata");
    
    // Test with a non-existent blob
    let blob_result = storage.get_blob(&unknown_cid).await?;
    assert!(blob_result.is_none(), "Blob should not exist");
    
    // Verify contains_blob returns false
    let contains_blob = storage.contains_blob(&unknown_cid).await?;
    assert!(!contains_blob, "Contains should return false for non-existent blob");
    
    Ok(())
}

#[tokio::test]
async fn test_overwrite_behavior() -> Result<()> {
    // Initialize storage manager
    let storage = InMemoryStorageManager::new();
    let entity_did = "did:icn:test_entity";
    
    // Create a test node
    let original_payload = Ipld::String("original data".to_string());
    let original_builder = TestNodeBuilder::new()
        .with_issuer(entity_did.to_string())
        .with_payload(original_payload.clone());
    
    // Store the original node
    let (original_cid, original_node) = storage.store_new_dag_root(entity_did, &original_builder).await?;
    
    // Create a different node with the same CID (manually insert to simulate overwrite)
    let updated_payload = Ipld::String("updated data".to_string());
    let mut updated_node = original_node.clone();
    updated_node.payload = updated_payload.clone();
    
    // Overwrite the node in the node store
    {
        let mut node_store = storage.node_store.lock().await;
        node_store.insert(original_cid, updated_node.clone());
    }
    
    // Serialize and store the updated node in the entity's namespace
    let serialized = dag_storage_codec().encode(&updated_node)?;
    let (did_key, node_key) = InMemoryStorageManager::node_key(entity_did, &original_cid);
    
    {
        let mut nodes = storage.nodes.lock().await;
        if let Some(entity_nodes) = nodes.get_mut(&did_key) {
            entity_nodes.insert(node_key, serialized);
        }
    }
    
    // Retrieve and verify the updated node is returned
    let retrieved_result = storage.get_node(entity_did, &original_cid).await?;
    assert!(retrieved_result.is_some(), "Node should exist");
    
    let retrieved_node = retrieved_result.unwrap();
    assert_eq!(retrieved_node.cid, original_cid, "CID should remain the same");
    assert_eq!(retrieved_node.payload, updated_payload, "Payload should be updated");
    assert_ne!(retrieved_node.payload, original_payload, "Payload should not match original");
    
    Ok(())
}

#[tokio::test]
async fn test_end_to_end_workflow() -> Result<()> {
    // This test validates a complete workflow using all storage features
    
    // Initialize storage manager
    let storage = InMemoryStorageManager::new();
    let entity_did = "did:icn:workflow_test";
    
    // 1. Create and store a root node
    let root_builder = TestNodeBuilder::with_random_content()
        .with_issuer(entity_did.to_string())
        .with_payload(Ipld::String("root node".to_string()));
    
    let (root_cid, root_node) = storage.store_new_dag_root(entity_did, &root_builder).await?;
    
    // 2. Create and store child nodes referencing the root
    let child1_builder = TestNodeBuilder::with_random_content()
        .with_issuer(entity_did.to_string())
        .with_parents(vec![root_cid])
        .with_payload(Ipld::String("child node 1".to_string()));
    
    let child2_builder = TestNodeBuilder::with_random_content()
        .with_issuer(entity_did.to_string())
        .with_parents(vec![root_cid])
        .with_payload(Ipld::String("child node 2".to_string()));
    
    // Store children
    let (child1_cid, child1_node) = storage.store_node(entity_did, &child1_builder).await?;
    let (child2_cid, child2_node) = storage.store_node(entity_did, &child2_builder).await?;
    
    // 3. Retrieve all nodes and verify relationships
    let retrieved_root = storage.get_node(entity_did, &root_cid).await?.unwrap();
    let retrieved_child1 = storage.get_node(entity_did, &child1_cid).await?.unwrap();
    let retrieved_child2 = storage.get_node(entity_did, &child2_cid).await?.unwrap();
    
    // Verify parent-child relationships
    assert!(retrieved_child1.parents.contains(&root_cid), "Child 1 should have root as parent");
    assert!(retrieved_child2.parents.contains(&root_cid), "Child 2 should have root as parent");
    
    // 4. Verify all nodes exist in storage
    assert!(storage.contains_node(entity_did, &root_cid).await?, "Root node should exist");
    assert!(storage.contains_node(entity_did, &child1_cid).await?, "Child 1 should exist");
    assert!(storage.contains_node(entity_did, &child2_cid).await?, "Child 2 should exist");
    
    // 5. Verify metadata accessibility
    let metadata_store = storage.metadata_store.lock().await;
    assert!(metadata_store.contains_key(&root_cid), "Root metadata should exist");
    assert!(metadata_store.contains_key(&child1_cid), "Child 1 metadata should exist");
    assert!(metadata_store.contains_key(&child2_cid), "Child 2 metadata should exist");
    
    // Verify metadata content
    let root_metadata = metadata_store.get(&root_cid).unwrap();
    assert_eq!(root_metadata.tags, root_node.metadata.tags, "Root metadata tags should match");
    
    // 6. Test raw data access via store_blob and get_blob
    let test_data = b"This is a raw blob test".to_vec();
    let blob_cid = storage.store_blob(&test_data).await?;
    
    let retrieved_blob = storage.get_blob(&blob_cid).await?;
    assert!(retrieved_blob.is_some(), "Blob should exist");
    assert_eq!(retrieved_blob.unwrap(), test_data, "Blob data should match");
    
    Ok(())
} 