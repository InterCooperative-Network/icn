use anyhow::Result;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use cid::Cid;
use futures::executor::block_on;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    MemoryStorageManager,
    StorageManager,
    AsyncInMemoryStorage,
    StorageBackend,
    StorageResult
};

#[cfg(test)]
mod storage_backend_tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestObj {
        name: String,
        value: i32,
    }

    #[tokio::test]
    async fn test_in_memory_storage_put_get() -> Result<()> {
        let storage = AsyncInMemoryStorage::new();
        
        // Create test data
        let data = b"hello world";
        
        // Store it
        let cid = storage.put_blob(data).await?;
        
        // Retrieve it
        let retrieved = storage.get_blob(&cid).await?;
        
        // Verify
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_commit() -> Result<()> {
        let storage = AsyncInMemoryStorage::new();
        
        // Begin transaction
        storage.begin_transaction().await?;
        
        // Store data in transaction
        let data = b"transaction test";
        let cid = storage.put_blob(data).await?;
        
        // Should be visible in transaction but not committed yet
        let retrieved_in_tx = storage.get_blob(&cid).await?;
        assert!(retrieved_in_tx.is_some());
        
        // Commit transaction
        storage.commit_transaction().await?;
        
        // Should be visible after commit
        let retrieved_after_commit = storage.get_blob(&cid).await?;
        assert!(retrieved_after_commit.is_some());
        assert_eq!(retrieved_after_commit.unwrap(), data);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_rollback() -> Result<()> {
        let storage = AsyncInMemoryStorage::new();
        
        // Store initial data outside transaction
        let initial_data = b"initial data";
        let initial_cid = storage.put_blob(initial_data).await?;
        
        // Begin transaction
        storage.begin_transaction().await?;
        
        // Store data in transaction
        let tx_data = b"transaction data";
        let tx_cid = storage.put_blob(tx_data).await?;
        
        // Delete initial data in transaction
        storage.delete_blob(&initial_cid).await?;
        
        // Verify transaction state
        assert!(storage.get_blob(&tx_cid).await?.is_some());
        assert!(storage.get_blob(&initial_cid).await?.is_none());
        
        // Rollback transaction
        storage.rollback_transaction().await?;
        
        // Transaction data should be gone, initial data should be back
        assert!(storage.get_blob(&tx_cid).await?.is_none());
        assert!(storage.get_blob(&initial_cid).await?.is_some());
        
        Ok(())
    }

    #[tokio::test]
    async fn test_key_value_operations() -> Result<()> {
        let storage = AsyncInMemoryStorage::new();
        
        // Create a key
        let mh = crate::create_sha256_multihash(b"test-key");
        let key_cid = Cid::new_v1(0x55, mh);
        
        // Store a value with the key
        let value = b"test-value".to_vec();
        storage.put_kv(key_cid, value.clone()).await?;
        
        // Check if the key exists
        assert!(storage.contains_kv(&key_cid).await?);
        
        // Get the value
        let retrieved = storage.get_kv(&key_cid).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
        
        // Delete the key
        storage.delete_kv(&key_cid).await?;
        
        // Verify it's gone
        assert!(!storage.contains_kv(&key_cid).await?);
        assert!(storage.get_kv(&key_cid).await?.is_none());
        
        Ok(())
    }
}

#[cfg(test)]
mod storage_manager_tests {
    use super::*;
    use icn_dag::{DagNodeBuilder, DagNodeMetadata};
    
    async fn create_test_node_builder(entity_did: &str, payload: serde_json::Value) -> Result<DagNodeBuilder> {
        let metadata = DagNodeMetadata {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            sequence: 1,
            content_type: Some("application/json".to_string()),
            tags: vec!["test".to_string()],
        };
        
        let builder = DagNodeBuilder::new()
            .with_issuer(entity_did.to_string())
            .with_metadata(metadata)
            .with_payload(libipld::Ipld::String(payload.to_string()));
            
        Ok(builder)
    }
    
    #[tokio::test]
    async fn test_memory_storage_manager() -> Result<()> {
        let storage = MemoryStorageManager::new();
        let entity_did = "did:icn:test-entity";
        
        // Create and store a root node
        let payload = serde_json::json!({ "type": "root", "name": "Test Entity" });
        let builder = create_test_node_builder(entity_did, payload).await?;
        let (root_cid, root_node) = storage.store_new_dag_root(entity_did, builder).await?;
        
        // Verify root node was stored
        let retrieved_root = storage.get_node(entity_did, &root_cid).await?;
        assert!(retrieved_root.is_some());
        assert_eq!(retrieved_root.unwrap().cid, root_node.cid);
        
        // Create and store a child node
        let child_payload = serde_json::json!({ "type": "child", "parent": root_cid.to_string() });
        let child_builder = create_test_node_builder(entity_did, child_payload).await?
            .with_parents(vec![root_cid]);
            
        let (child_cid, child_node) = storage.store_node(entity_did, child_builder).await?;
        
        // Verify child node was stored
        let retrieved_child = storage.get_node(entity_did, &child_cid).await?;
        assert!(retrieved_child.is_some());
        let child = retrieved_child.unwrap();
        assert_eq!(child.cid, child_node.cid);
        assert_eq!(child.parents, vec![root_cid]);
        
        // Test contains_node
        assert!(storage.contains_node(entity_did, &root_cid).await?);
        assert!(storage.contains_node(entity_did, &child_cid).await?);
        assert!(!storage.contains_node(entity_did, &Cid::default()).await?);
        
        // Test batch storage
        let batch_builders = vec![
            create_test_node_builder(entity_did, serde_json::json!({ "type": "batch", "id": 1 })).await?,
            create_test_node_builder(entity_did, serde_json::json!({ "type": "batch", "id": 2 })).await?,
        ];
        
        let batch_results = storage.store_nodes_batch(entity_did, batch_builders).await?;
        assert_eq!(batch_results.len(), 2);
        
        // Verify batch nodes were stored
        for (cid, _) in &batch_results {
            assert!(storage.contains_node(entity_did, cid).await?);
        }
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_entity_isolation() -> Result<()> {
        let storage = MemoryStorageManager::new();
        let entity1_did = "did:icn:entity1";
        let entity2_did = "did:icn:entity2";
        
        // Create root nodes for both entities
        let builder1 = create_test_node_builder(entity1_did, serde_json::json!({ "entity": 1 })).await?;
        let builder2 = create_test_node_builder(entity2_did, serde_json::json!({ "entity": 2 })).await?;
        
        let (cid1, _) = storage.store_new_dag_root(entity1_did, builder1).await?;
        let (cid2, _) = storage.store_new_dag_root(entity2_did, builder2).await?;
        
        // Verify nodes are isolated to their respective entities
        assert!(storage.contains_node(entity1_did, &cid1).await?);
        assert!(!storage.contains_node(entity1_did, &cid2).await?);
        
        assert!(storage.contains_node(entity2_did, &cid2).await?);
        assert!(!storage.contains_node(entity2_did, &cid1).await?);
        
        Ok(())
    }
} 