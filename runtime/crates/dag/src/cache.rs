/*!
# DAG Caching

Provides caching mechanisms for DAG operations to improve performance under load.
*/

use crate::DagNode;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use cid::Cid;
use crate::create_sha256_multihash;

/// Cache for DAG nodes to improve performance
pub struct DagNodeCache {
    /// LRU cache for nodes
    cache: Mutex<LruCache<Cid, Arc<DagNode>>>,
    /// Stats for cache hits/misses
    stats: Mutex<CacheStats>,
}

/// Statistics for cache hits/misses
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: usize,
    /// Number of cache misses
    pub misses: usize,
    /// Number of cache insertions
    pub insertions: usize,
}

impl CacheStats {
    /// Calculate the hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

impl DagNodeCache {
    /// Create a new DAG node cache with the specified capacity
    pub fn new(capacity: usize) -> Self {
        // Ensure capacity is at least 1
        let capacity = std::cmp::max(1, capacity);
        let cache = Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap()));
        let stats = Mutex::new(CacheStats::default());
        
        Self { cache, stats }
    }
    
    /// Get a node from the cache, if present
    pub fn get(&self, cid: &Cid) -> Option<Arc<DagNode>> {
        let mut cache = self.cache.lock().unwrap();
        let result = cache.get(cid).cloned();
        
        // Update stats
        let mut stats = self.stats.lock().unwrap();
        if result.is_some() {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
        
        result
    }
    
    /// Insert a node into the cache
    pub fn insert(&self, cid: Cid, node: Arc<DagNode>) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(cid, node);
        
        // Update stats
        let mut stats = self.stats.lock().unwrap();
        stats.insertions += 1;
    }
    
    /// Remove a node from the cache
    pub fn remove(&self, cid: &Cid) {
        let mut cache = self.cache.lock().unwrap();
        cache.pop(cid);
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap();
        stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::{create_sha256_multihash, DagNode, DagNodeMetadata, IdentityId, Signature};

    #[test]
    fn test_cache_insertion_retrieval() {
        let cache = DagNodeCache::new(10);
        
        // Generate a test CID
        let data = b"test";
        let mh = create_sha256_multihash(data);
        let cid = Cid::new_v1(0x71, mh);
        
        let test_node = Arc::new(DagNode {
            cid: Some(cid),
            content: data.to_vec(),
            parents: vec![],
            signer: IdentityId("did:icn:test".to_string()),
            signature: Signature(vec![1, 2, 3, 4]),
            metadata: DagNodeMetadata::default(),
        });
        
        // Insert the node
        cache.insert(cid, test_node.clone());
        
        // Retrieve the node
        let retrieved = cache.get(&cid);
        assert!(retrieved.is_some());
        
        let retrieved_node = retrieved.unwrap();
        assert_eq!(retrieved_node.content, data.to_vec());
    }
    
    #[test]
    fn test_cache_capacity() {
        let cache = DagNodeCache::new(2);
        
        // Create three test nodes
        let node1_data = b"node1";
        let mh1 = create_sha256_multihash(node1_data);
        let cid1 = Cid::new_v1(0x71, mh1);
        
        let node1 = Arc::new(DagNode {
            cid: Some(cid1),
            content: node1_data.to_vec(),
            parents: vec![],
            signer: IdentityId("did:icn:test".to_string()),
            signature: Signature(vec![1, 2, 3, 4]),
            metadata: DagNodeMetadata::default(),
        });
        
        let node2_data = b"node2";
        let mh2 = create_sha256_multihash(node2_data);
        let cid2 = Cid::new_v1(0x71, mh2);
        
        let node2 = Arc::new(DagNode {
            cid: Some(cid2),
            content: node2_data.to_vec(),
            parents: vec![],
            signer: IdentityId("did:icn:test".to_string()),
            signature: Signature(vec![1, 2, 3, 4]),
            metadata: DagNodeMetadata::default(),
        });
        
        let node3_data = b"node3";
        let mh3 = create_sha256_multihash(node3_data);
        let cid3 = Cid::new_v1(0x71, mh3);
        
        let node3 = Arc::new(DagNode {
            cid: Some(cid3),
            content: node3_data.to_vec(),
            parents: vec![],
            signer: IdentityId("did:icn:test".to_string()),
            signature: Signature(vec![1, 2, 3, 4]),
            metadata: DagNodeMetadata::default(),
        });
        
        // Insert the first two nodes
        cache.insert(cid1, node1.clone());
        cache.insert(cid2, node2.clone());
        
        // Verify we can get both nodes
        assert!(cache.get(&cid1).is_some());
        assert!(cache.get(&cid2).is_some());
        
        // Insert the third node, which should evict the least recently used node (cid1)
        cache.insert(cid3, node3.clone());
        
        // Verify cid1 is no longer in the cache
        assert!(cache.get(&cid1).is_none());
        
        // But cid2 and cid3 should be there
        assert!(cache.get(&cid2).is_some());
        assert!(cache.get(&cid3).is_some());
    }
} 