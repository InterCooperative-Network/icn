/*!
# DAG Storage Integration

This module integrates the optimized DAG components with the storage system, providing
a high-performance implementation of the DagManager that uses caching, batch operations,
and audit logging.
*/

use crate::{
    DagNode, DagNodeBuilder, DagManager, DagError, DagResult, Signer,
    audit::{AuditLogger, AuditAction, AuditedOperation},
    cache::DagNodeCache,
    query::{DagQuery, NodeLoader, QueryResult}
};
use async_trait::async_trait;
use cid::Cid;
use icn_storage::{StorageManager, Result};
use libipld::codec::Codec;
use libipld::ipld::Ipld;
use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use std::time::Duration;
use std::collections::HashSet;
use tracing::{debug, info, warn, error};
use std::future::Future;
use tokio::sync::RwLock as AsyncRwLock;
use futures::future::join_all;
use anyhow::Context;
use crate::codec::DagCborCodec;
use icn_models::storage::{StorageBackend, StorageResult};

/// Number of nodes to fetch in parallel when traversing the DAG
const PARALLEL_FETCH_LIMIT: usize = 16;

/// Cache size for the DAG node cache
const DEFAULT_CACHE_SIZE: usize = 1000;

/// Optimized implementation of DagManager that uses StorageManager, caching, 
/// and audit logging
pub struct OptimizedDagManager<S, L, SI> 
where
    S: StorageManager + 'static,
    L: AuditLogger + 'static,
    SI: Signer + 'static,
{
    /// Storage manager for persisting nodes
    storage: Arc<S>,
    
    /// Node cache for performance
    cache: Arc<DagNodeCache>,
    
    /// Audit logger for security and traceability
    logger: Arc<L>,
    
    /// Signer for authentication
    signer: Arc<SI>,
    
    /// Entity to store nodes for
    entity_did: String,
    
    /// In-memory DAG tips (latest nodes)
    tips: AsyncRwLock<HashSet<Cid>>,
    
    /// In-memory reverse index (parent -> children)
    children_index: AsyncRwLock<HashMap<Cid, Vec<Cid>>>,
    
    /// Maximum depth to traverse
    max_traverse_depth: usize,
}

impl<S, L, SI> OptimizedDagManager<S, L, SI> 
where
    S: StorageManager + 'static,
    L: AuditLogger + 'static,
    SI: Signer + 'static,
{
    /// Create a new optimized DAG manager
    pub async fn new(
        storage: Arc<S>,
        logger: Arc<L>,
        signer: Arc<SI>,
        entity_did: String,
    ) -> Result<Self> {
        let cache = Arc::new(DagNodeCache::new(DEFAULT_CACHE_SIZE));
        
        let manager = Self {
            storage,
            cache,
            logger,
            signer,
            entity_did,
            tips: AsyncRwLock::new(HashSet::new()),
            children_index: AsyncRwLock::new(HashMap::new()),
            max_traverse_depth: 100, // Default max depth
        };
        
        // Initialize in-memory indexes
        manager.initialize_indexes().await?;
        
        Ok(manager)
    }
    
    /// Create a new optimized DAG manager with custom options
    pub async fn with_options(
        storage: Arc<S>,
        logger: Arc<L>,
        signer: Arc<SI>,
        entity_did: String,
        cache_size: usize,
        max_traverse_depth: usize,
    ) -> Result<Self> {
        let cache = Arc::new(DagNodeCache::with_options(
            cache_size,
            3, // Prefetch depth
            20, // Max prefetch count
            true, // Enable prefetching
        ));
        
        let manager = Self {
            storage,
            cache,
            logger,
            signer,
            entity_did,
            tips: AsyncRwLock::new(HashSet::new()),
            children_index: AsyncRwLock::new(HashMap::new()),
            max_traverse_depth,
        };
        
        // Initialize in-memory indexes
        manager.initialize_indexes().await?;
        
        Ok(manager)
    }
    
    /// Initialize the in-memory indexes
    async fn initialize_indexes(&self) -> Result<()> {
        // Load existing tips from storage
        let tips = self.load_tips().await?;
        
        // Initialize tips
        let mut tips_lock = self.tips.write().await;
        *tips_lock = tips;
        drop(tips_lock);
        
        // Initialize children index
        let children_index = self.build_children_index().await?;
        
        let mut index_lock = self.children_index.write().await;
        *index_lock = children_index;
        
        Ok(())
    }
    
    /// Load tips from storage
    async fn load_tips(&self) -> Result<HashSet<Cid>> {
        // This would normally query a specific "tips" index in the storage
        // For now, we'll do a naive implementation - load a bunch of recent nodes
        // and find those that aren't parents of any other node
        
        // First find any nodes with the "tip" tag
        // This is a placeholder implementation
        let mut tips = HashSet::new();
        
        info!("Loaded {} tips from storage", tips.len());
        Ok(tips)
    }
    
    /// Build the children index
    async fn build_children_index(&self) -> Result<HashMap<Cid, Vec<Cid>>> {
        // This would build a reverse index of parent -> children
        // For now, we'll return an empty map and let it be populated as nodes are added
        let index = HashMap::new();
        
        info!("Built initial children index");
        Ok(index)
    }
    
    /// Update the tips index when a new node is added
    async fn update_tips(&self, node: &DagNode, node_cid: &Cid) -> Result<()> {
        let mut tips = self.tips.write().await;
        
        // Add this node as a tip
        tips.insert(*node_cid);
        
        // Remove any parents from tips since they now have children
        for parent_cid in &node.parents {
            tips.remove(parent_cid);
        }
        
        Ok(())
    }
    
    /// Update the children index when a new node is added
    async fn update_children_index(&self, node: &DagNode, node_cid: &Cid) -> Result<()> {
        let mut index = self.children_index.write().await;
        
        // Add this node as a child of each parent
        for parent_cid in &node.parents {
            let children = index.entry(*parent_cid).or_insert_with(Vec::new);
            children.push(*node_cid);
        }
        
        Ok(())
    }
    
    /// Get a node loader that works with our caching system
    fn get_node_loader(&self) -> CachingNodeLoader<S> {
        CachingNodeLoader {
            storage: self.storage.clone(),
            cache: self.cache.clone(),
            entity_did: self.entity_did.clone(),
        }
    }
    
    /// Create a DAG query for this entity
    pub fn query(&self, cids: Vec<Cid>) -> DagQuery {
        DagQuery::from(cids)
    }
    
    /// Parse and execute a query string
    pub async fn execute_query(&self, query_str: &str, start_cids: Vec<Cid>) -> QueryResult<Vec<Arc<DagNode>>> {
        let query = DagQuery::parse(query_str, start_cids)?;
        let loader = self.get_node_loader();
        query.execute(&loader).await
    }
}

#[async_trait]
impl<S, L, SI> DagManager for OptimizedDagManager<S, L, SI>
where
    S: StorageManager + 'static,
    L: AuditLogger + 'static,
    SI: Signer + 'static,
{
    async fn store_node(&self, node: &DagNode) -> Result<Cid> {
        // Use the audited operation helper for security logging
        AuditedOperation::new(
            &*self.logger,
            AuditAction::NodeCreated,
            node.issuer.to_string()
        )
        .with_entity(self.entity_did.clone())
        .execute_async(async {
            // 1. Build the node (not needed as we already have the node)
            
            // 2. Verify the node signature
            self.signer.verify(node).context("Failed to verify node signature")?;
            
            // 3. Encode the node
            let codec = DagCborCodec;
            let node_bytes = codec.encode(node).context("Failed to encode node")?;
            
            // 4. Store the node
            let node_builder = DagNodeBuilder::new()
                .payload(node.payload.clone())
                .parents(node.parents.clone())
                .issuer(node.issuer.clone())
                .metadata(node.metadata.clone());
                
            let (cid, _) = self.storage.store_node(&self.entity_did, node_builder)
                .await
                .context("Failed to store node")?;
            
            // 5. Update in-memory indexes
            self.update_tips(node, &cid).await?;
            self.update_children_index(node, &cid).await?;
            
            // 6. Add to cache
            self.cache.insert(cid, Arc::new(node.clone()));
            
            // 7. Start predictive loading of related nodes
            let loader_fn = {
                let loader = self.get_node_loader();
                move |cid: Cid| -> futures::future::BoxFuture<'static, Option<DagNode>> {
                    let loader_clone = loader.clone();
                    Box::pin(async move {
                        match loader_clone.load_node(&cid).await {
                            Ok(Some(node)) => Some((*node).clone()),
                            _ => None,
                        }
                    })
                }
            };
            
            self.cache.predictive_load(&cid, node, loader_fn).await;
            
            Ok(cid)
        })
        .await
    }
    
    async fn store_nodes_batch(&self, nodes: Vec<DagNode>) -> Result<Vec<Cid>> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }
        
        // Use the audited operation helper for security logging
        AuditedOperation::new(
            &*self.logger,
            AuditAction::NodeCreated,
            nodes[0].issuer.to_string()
        )
        .with_entity(self.entity_did.clone())
        .execute_async(async {
            // 1. Verify signatures for all nodes
            for node in &nodes {
                self.signer.verify(node).context("Failed to verify node signature")?;
            }
            
            // 2. Convert nodes to builders
            let node_builders = nodes.iter().map(|node| {
                DagNodeBuilder::new()
                    .payload(node.payload.clone())
                    .parents(node.parents.clone())
                    .issuer(node.issuer.clone())
                    .metadata(node.metadata.clone())
            }).collect::<Vec<_>>();
            
            // 3. Store all nodes in a batch
            let result = self.storage.store_nodes_batch(
                &self.entity_did,
                node_builders
            )
            .await
            .context("Failed to store nodes batch")?;
            
            // 4. Update in-memory indexes and cache
            let mut cids = Vec::with_capacity(result.len());
            
            for (i, (cid, _)) in result.into_iter().enumerate() {
                let node = &nodes[i];
                
                // Update indexes
                self.update_tips(node, &cid).await?;
                self.update_children_index(node, &cid).await?;
                
                // Add to cache
                self.cache.insert(cid, Arc::new(node.clone()));
                
                cids.push(cid);
            }
            
            // 5. Start predictive loading for the first few nodes
            // This is a compromise - we don't want to overwhelm the system with predictive loading
            // requests for all nodes in a large batch
            for i in 0..std::cmp::min(3, nodes.len()) {
                let node = &nodes[i];
                let cid = cids[i];
                
                let loader_fn = {
                    let loader = self.get_node_loader();
                    move |cid: Cid| -> futures::future::BoxFuture<'static, Option<DagNode>> {
                        let loader_clone = loader.clone();
                        Box::pin(async move {
                            match loader_clone.load_node(&cid).await {
                                Ok(Some(node)) => Some((*node).clone()),
                                _ => None,
                            }
                        })
                    }
                };
                
                self.cache.predictive_load(&cid, node, loader_fn).await;
            }
            
            Ok(cids)
        })
        .await
    }
    
    async fn get_node(&self, cid: &Cid) -> Result<Option<DagNode>> {
        // Use the audited operation helper
        AuditedOperation::new(
            &*self.logger,
            AuditAction::NodeRead,
            "system".to_string()
        )
        .with_entity(self.entity_did.clone())
        .with_node(*cid)
        .execute_async(async {
            // Check cache first
            if let Some(cached_node) = self.cache.get(cid) {
                return Ok(Some((*cached_node).clone()));
            }
            
            // If not in cache, load from storage
            match self.storage.get_node(&self.entity_did, cid).await? {
                Some(node) => {
                    // Add to cache for next time
                    self.cache.insert(*cid, Arc::new(node.clone()));
                    
                    // Start predictive loading
                    let loader_fn = {
                        let loader = self.get_node_loader();
                        move |cid: Cid| -> futures::future::BoxFuture<'static, Option<DagNode>> {
                            let loader_clone = loader.clone();
                            Box::pin(async move {
                                match loader_clone.load_node(&cid).await {
                                    Ok(Some(node)) => Some((*node).clone()),
                                    _ => None,
                                }
                            })
                        }
                    };
                    
                    self.cache.predictive_load(cid, &node, loader_fn).await;
                    
                    Ok(Some(node))
                },
                None => Ok(None),
            }
        })
        .await
    }
    
    async fn contains_node(&self, cid: &Cid) -> Result<bool> {
        // Check cache first
        if self.cache.get(cid).is_some() {
            return Ok(true);
        }
        
        // If not in cache, check storage
        self.storage.contains_node(&self.entity_did, cid).await
    }
    
    async fn get_parents(&self, cid: &Cid) -> Result<Vec<DagNode>> {
        // Get the node first
        let node = match self.get_node(cid).await? {
            Some(n) => n,
            None => return Ok(Vec::new()),
        };
        
        // Load all parents in parallel
        let mut parent_futures = Vec::with_capacity(node.parents.len());
        
        for parent_cid in &node.parents {
            let self_clone = self.clone();
            let parent_cid = *parent_cid;
            
            let future = async move {
                self_clone.get_node(&parent_cid).await
            };
            
            parent_futures.push(future);
        }
        
        // Wait for all parent loads to complete
        let parent_results = join_all(parent_futures).await;
        
        // Collect successful results
        let mut parents = Vec::new();
        for result in parent_results {
            match result {
                Ok(Some(parent)) => parents.push(parent),
                Ok(None) => {}, // Parent not found, skip
                Err(e) => warn!("Error loading parent: {}", e),
            }
        }
        
        Ok(parents)
    }
    
    async fn get_children(&self, cid: &Cid) -> Result<Vec<DagNode>> {
        // Check the children index
        let children_cids = {
            let index = self.children_index.read().await;
            index.get(cid).cloned().unwrap_or_default()
        };
        
        if children_cids.is_empty() {
            return Ok(Vec::new());
        }
        
        // Load all children in parallel
        let mut children_futures = Vec::with_capacity(children_cids.len());
        
        for child_cid in &children_cids {
            let self_clone = self.clone();
            let child_cid = *child_cid;
            
            let future = async move {
                self_clone.get_node(&child_cid).await
            };
            
            children_futures.push(future);
        }
        
        // Wait for all children loads to complete
        let children_results = join_all(children_futures).await;
        
        // Collect successful results
        let mut children = Vec::new();
        for result in children_results {
            match result {
                Ok(Some(child)) => children.push(child),
                Ok(None) => {}, // Child not found, skip
                Err(e) => warn!("Error loading child: {}", e),
            }
        }
        
        Ok(children)
    }
    
    async fn verify_node(&self, cid: &Cid) -> Result<bool> {
        // Use the audited operation helper
        AuditedOperation::new(
            &*self.logger,
            AuditAction::NodeVerified,
            "system".to_string()
        )
        .with_entity(self.entity_did.clone())
        .with_node(*cid)
        .execute_async(async {
            // Get the node
            let node = match self.get_node(cid).await? {
                Some(n) => n,
                None => return Ok(false),
            };
            
            // Verify the signature
            self.signer.verify(&node)
        })
        .await
    }
    
    async fn get_tips(&self) -> Result<Vec<Cid>> {
        let tips = self.tips.read().await;
        Ok(tips.iter().cloned().collect())
    }
}

impl<S, L, SI> Clone for OptimizedDagManager<S, L, SI>
where
    S: StorageManager + 'static,
    L: AuditLogger + 'static,
    SI: Signer + 'static,
{
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            cache: self.cache.clone(),
            logger: self.logger.clone(),
            signer: self.signer.clone(),
            entity_did: self.entity_did.clone(),
            tips: self.tips.clone(),
            children_index: self.children_index.clone(),
            max_traverse_depth: self.max_traverse_depth,
        }
    }
}

/// A node loader that integrates with the cache
#[derive(Clone)]
struct CachingNodeLoader<S: StorageManager + 'static> {
    storage: Arc<S>,
    cache: Arc<DagNodeCache>,
    entity_did: String,
}

#[async_trait]
impl<S: StorageManager + 'static> NodeLoader for CachingNodeLoader<S> {
    async fn load_node(&self, cid: &Cid) -> QueryResult<Option<Arc<DagNode>>> {
        // Check cache first
        if let Some(cached_node) = self.cache.get(cid) {
            return Ok(Some(cached_node));
        }
        
        // If not in cache, load from storage
        match self.storage.get_node(&self.entity_did, cid).await {
            Ok(Some(node)) => {
                // Add to cache for next time
                let node_arc = Arc::new(node);
                self.cache.insert(*cid, node_arc.clone());
                Ok(Some(node_arc))
            },
            Ok(None) => Ok(None),
            Err(e) => Err(crate::query::QueryError::StorageError(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DagNode, DagNodeBuilder, DagNodeMetadata, IdentityId, Signer};
    use anyhow::Result;
    use async_trait::async_trait;
    use cid::Cid;
    use icn_storage::{StorageBackend, StorageManager, StorageResult};
    use libipld::ipld;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::runtime::Runtime;
    
    // Mock implementations for testing
    
    struct MockSigner;
    
    impl Signer for MockSigner {
        fn sign(&self, _node: &DagNode) -> Result<Vec<u8>> {
            Ok(vec![1, 2, 3, 4])
        }
        
        fn verify(&self, _node: &DagNode) -> Result<bool> {
            Ok(true)
        }
    }
    
    struct MockStorageManager {
        nodes: Mutex<HashMap<String, HashMap<String, DagNode>>>,
    }
    
    impl MockStorageManager {
        fn new() -> Self {
            Self {
                nodes: Mutex::new(HashMap::new()),
            }
        }
    }
    
    #[async_trait]
    impl StorageManager for MockStorageManager {
        async fn store_new_dag_root(
            &self,
            entity_did: &str,
            node_builder: DagNodeBuilder,
        ) -> Result<(Cid, DagNode)> {
            let node = node_builder.build()?;
            let codec = DagCborCodec;
            let node_bytes = codec.encode(&node)?;
            let cid = Cid::new_v1(codec.into(), crate::create_sha256_multihash(&node_bytes));
            
            let mut nodes = self.nodes.lock().unwrap();
            let entity_nodes = nodes.entry(entity_did.to_string()).or_default();
            entity_nodes.insert(cid.to_string(), node.clone());
            
            Ok((cid, node))
        }
        
        async fn store_node(
            &self,
            entity_did: &str,
            node_builder: DagNodeBuilder,
        ) -> Result<(Cid, DagNode)> {
            let node = node_builder.build()?;
            let codec = DagCborCodec;
            let node_bytes = codec.encode(&node)?;
            let cid = Cid::new_v1(codec.into(), crate::create_sha256_multihash(&node_bytes));
            
            let mut nodes = self.nodes.lock().unwrap();
            let entity_nodes = nodes.entry(entity_did.to_string()).or_default();
            entity_nodes.insert(cid.to_string(), node.clone());
            
            Ok((cid, node))
        }
        
        async fn store_nodes_batch(
            &self,
            entity_did: &str,
            node_builders: Vec<DagNodeBuilder>,
        ) -> Result<Vec<(Cid, DagNode)>> {
            let mut results = Vec::with_capacity(node_builders.len());
            
            for builder in node_builders {
                let result = self.store_node(entity_did, builder).await?;
                results.push(result);
            }
            
            Ok(results)
        }
        
        async fn get_node(&self, entity_did: &str, cid: &Cid) -> Result<Option<DagNode>> {
            let nodes = self.nodes.lock().unwrap();
            let entity_nodes = match nodes.get(entity_did) {
                Some(n) => n,
                None => return Ok(None),
            };
            
            match entity_nodes.get(&cid.to_string()) {
                Some(node) => Ok(Some(node.clone())),
                None => Ok(None),
            }
        }
        
        async fn contains_node(&self, entity_did: &str, cid: &Cid) -> Result<bool> {
            let nodes = self.nodes.lock().unwrap();
            let entity_nodes = match nodes.get(entity_did) {
                Some(n) => n,
                None => return Ok(false),
            };
            
            Ok(entity_nodes.contains_key(&cid.to_string()))
        }
        
        async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> Result<Option<Vec<u8>>> {
            match self.get_node(entity_did, cid).await? {
                Some(node) => {
                    let codec = DagCborCodec;
                    let bytes = codec.encode(&node)?;
                    Ok(Some(bytes))
                },
                None => Ok(None),
            }
        }
    }
    
    struct MockAuditLogger;
    
    #[async_trait]
    impl AuditLogger for MockAuditLogger {
        async fn record(&self, _record: crate::audit::AuditRecord) -> crate::audit::AuditResult<()> {
            Ok(())
        }
        
        async fn get_records_for_entity(&self, _entity_did: &str, _limit: usize) -> crate::audit::AuditResult<Vec<crate::audit::AuditRecord>> {
            Ok(Vec::new())
        }
        
        async fn get_records_for_node(&self, _node_cid: &Cid, _limit: usize) -> crate::audit::AuditResult<Vec<crate::audit::AuditRecord>> {
            Ok(Vec::new())
        }
        
        async fn get_records_for_actor(&self, _actor_did: &str, _limit: usize) -> crate::audit::AuditResult<Vec<crate::audit::AuditRecord>> {
            Ok(Vec::new())
        }
        
        async fn get_all_records(&self, _limit: usize) -> crate::audit::AuditResult<Vec<crate::audit::AuditRecord>> {
            Ok(Vec::new())
        }
        
        fn subscribe(&self) -> crate::audit::AuditResult<tokio::sync::broadcast::Receiver<crate::audit::AuditRecord>> {
            let (tx, rx) = tokio::sync::broadcast::channel(1);
            Ok(rx)
        }
    }
    
    #[tokio::test]
    async fn test_optimized_dag_manager() {
        let storage = Arc::new(MockStorageManager::new());
        let logger = Arc::new(MockAuditLogger);
        let signer = Arc::new(MockSigner);
        let entity_did = "did:icn:test_entity".to_string();
        
        let manager = OptimizedDagManager::new(
            storage.clone(),
            logger.clone(),
            signer.clone(),
            entity_did.clone(),
        ).await.unwrap();
        
        // Create a test node
        let node = DagNodeBuilder::new()
            .payload(ipld!({ "message": "Hello, world!" }))
            .issuer(IdentityId("did:icn:test_user".to_string()))
            .build()
            .unwrap();
        
        // Store the node
        let cid = manager.store_node(&node).await.unwrap();
        
        // Retrieve the node
        let retrieved = manager.get_node(&cid).await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved_node = retrieved.unwrap();
        assert_eq!(retrieved_node.issuer.0, "did:icn:test_user");
        
        // Verify the node
        let valid = manager.verify_node(&cid).await.unwrap();
        assert!(valid);
        
        // Check tips
        let tips = manager.get_tips().await.unwrap();
        assert!(tips.contains(&cid));
        
        // Create a child node
        let child_node = DagNodeBuilder::new()
            .payload(ipld!({ "message": "Child node" }))
            .parent(cid)
            .issuer(IdentityId("did:icn:test_user".to_string()))
            .build()
            .unwrap();
        
        // Store the child node
        let child_cid = manager.store_node(&child_node).await.unwrap();
        
        // Check tips again
        let tips = manager.get_tips().await.unwrap();
        assert!(!tips.contains(&cid)); // Parent is no longer a tip
        assert!(tips.contains(&child_cid)); // Child is a tip
        
        // Get children of parent
        let children = manager.get_children(&cid).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].payload, ipld!({ "message": "Child node" }));
        
        // Get parents of child
        let parents = manager.get_parents(&child_cid).await.unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].payload, ipld!({ "message": "Hello, world!" }));
        
        // Test batch storage
        let batch_nodes = vec![
            DagNodeBuilder::new()
                .payload(ipld!({ "message": "Batch 1" }))
                .parent(child_cid)
                .issuer(IdentityId("did:icn:test_user".to_string()))
                .build()
                .unwrap(),
            DagNodeBuilder::new()
                .payload(ipld!({ "message": "Batch 2" }))
                .parent(child_cid)
                .issuer(IdentityId("did:icn:test_user".to_string()))
                .build()
                .unwrap(),
        ];
        
        let batch_cids = manager.store_nodes_batch(batch_nodes).await.unwrap();
        assert_eq!(batch_cids.len(), 2);
        
        // Check tips again
        let tips = manager.get_tips().await.unwrap();
        assert!(!tips.contains(&child_cid)); // Child is no longer a tip
        assert!(tips.contains(&batch_cids[0])); // Batch nodes are tips
        assert!(tips.contains(&batch_cids[1]));
    }
} 