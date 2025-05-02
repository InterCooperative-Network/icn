/*!
# DAG Caching

Provides caching mechanisms for DAG operations to improve performance under load.
*/

use crate::DagNode;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use cid::Cid;

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
    use cid::Version;
    use crate::{DagNode, DagNodeMetadata, IdentityId, Signature};
    use multihash::{Code, MultihashDigest};
    
    // Helper function to create a test DagNode
    fn create_test_node(data: &[u8]) -> DagNode {
        DagNode {
            cid: None,
            content: data.to_vec(),
            parents: vec![],
            signer: IdentityId("did:icn:test".to_string()),
            signature: Signature(vec![1, 2, 3, 4]),
            metadata: DagNodeMetadata::new(),
        }
    }
    
    #[test]
    fn test_cache_basic_operations() {
        let cache = DagNodeCache::new(100);
        
        // Create a test node
        let node = Arc::new(create_test_node(b"test data"));
        
        // Create a test CID
        let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(b"test"));
        
        // Initially the node should not be in the cache
        assert!(cache.get(&cid).is_none());
        
        // Insert the node
        cache.insert(cid, node.clone());
        
        // Now it should be in the cache
        let cached_node = cache.get(&cid);
        assert!(cached_node.is_some());
        assert_eq!(cached_node.unwrap().content, node.content);
        
        // Remove it from the cache
        cache.remove(&cid);
        
        // Now it should not be in the cache again
        assert!(cache.get(&cid).is_none());
        
        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.insertions, 1);
    }
    
    #[test]
    fn test_cache_lru_behavior() {
        // Create a cache with capacity 2
        let cache = DagNodeCache::new(2);
        
        // Create three test nodes
        let node1 = Arc::new(create_test_node(b"node1"));
        let cid1 = Cid::new_v1(0x71, Code::Sha2_256.digest(b"node1"));
        
        let node2 = Arc::new(create_test_node(b"node2"));
        let cid2 = Cid::new_v1(0x71, Code::Sha2_256.digest(b"node2"));
        
        let node3 = Arc::new(create_test_node(b"node3"));
        let cid3 = Cid::new_v1(0x71, Code::Sha2_256.digest(b"node3"));
        
        // Insert first two nodes
        cache.insert(cid1, node1.clone());
        cache.insert(cid2, node2.clone());
        
        // Both should be in the cache
        assert!(cache.get(&cid1).is_some());
        assert!(cache.get(&cid2).is_some());
        
        // Insert third node, which should evict the first one (LRU)
        cache.insert(cid3, node3.clone());
        
        // Now node1 should be evicted, but node2 and node3 should be present
        assert!(cache.get(&cid1).is_none());
        assert!(cache.get(&cid2).is_some());
        assert!(cache.get(&cid3).is_some());
        
        // Access node2, making node3 the LRU
        cache.get(&cid2);
        
        // Insert node1 again, which should evict node3
        cache.insert(cid1, node1.clone());
        
        // Now node3 should be evicted, but node1 and node2 should be present
        assert!(cache.get(&cid1).is_some());
        assert!(cache.get(&cid2).is_some());
        assert!(cache.get(&cid3).is_none());
    }
} 