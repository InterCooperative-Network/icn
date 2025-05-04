/*!
# DAG Caching

Provides caching mechanisms for DAG operations to improve performance under load.
*/

use crate::DagNode;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, RwLock};
use cid::Cid;
use crate::create_sha256_multihash;
use std::collections::{HashMap, HashSet, VecDeque};
use tracing::{debug, info, warn};
use futures::future::join_all;
use std::time::{Duration, Instant};

/// Cache for DAG nodes to improve performance
pub struct DagNodeCache {
    /// LRU cache for nodes
    cache: RwLock<LruCache<Cid, Arc<DagNode>>>,
    /// Stats for cache hits/misses
    stats: Mutex<CacheStats>,
    /// Prefetch tracker to avoid redundant prefetches
    prefetch_tracker: Mutex<HashSet<Cid>>,
    /// Access patterns for predictive loading
    access_patterns: Mutex<AccessPatternTracker>,
    /// Maximum depth for prefetching (how many levels of links to follow)
    max_prefetch_depth: usize,
    /// Maximum number of nodes to prefetch in a single operation
    max_prefetch_count: usize,
    /// Prefetch feature toggle
    prefetch_enabled: bool,
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
    /// Number of predictive loads triggered
    pub predictive_loads: usize,
    /// Number of nodes prefetched
    pub prefetched_nodes: usize,
    /// Number of prefetched nodes that were subsequently accessed
    pub prefetch_hits: usize,
}

/// Track access patterns for predictive loading
struct AccessPatternTracker {
    /// Recent access sequence (CID -> timestamp)
    recent_accesses: VecDeque<(Cid, Instant)>,
    /// Co-access patterns: when CID A is accessed, which CIDs are frequently accessed afterward
    co_access_patterns: HashMap<Cid, HashMap<Cid, usize>>,
    /// Maximum size of recent accesses queue
    max_recent_accesses: usize,
    /// Timestamp of the last cleanup
    last_cleanup: Instant,
    /// Minimum count to consider a pattern significant
    min_pattern_count: usize,
}

impl AccessPatternTracker {
    /// Create a new access pattern tracker
    fn new(max_recent_accesses: usize, min_pattern_count: usize) -> Self {
        Self {
            recent_accesses: VecDeque::with_capacity(max_recent_accesses),
            co_access_patterns: HashMap::new(),
            max_recent_accesses,
            last_cleanup: Instant::now(),
            min_pattern_count,
        }
    }
    
    /// Record a CID access
    fn record_access(&mut self, cid: &Cid) {
        let now = Instant::now();
        
        // Record this access
        self.recent_accesses.push_back((*cid, now));
        
        // Update co-access patterns with recent accesses
        // (any CID accessed in the last 500ms is considered related)
        let recent_window = Duration::from_millis(500);
        for (prev_cid, prev_time) in self.recent_accesses.iter().rev().skip(1) {
            if now.duration_since(*prev_time) <= recent_window {
                let pattern_count = self.co_access_patterns
                    .entry(*prev_cid)
                    .or_insert_with(HashMap::new)
                    .entry(*cid)
                    .or_insert(0);
                *pattern_count += 1;
            } else {
                break; // Outside the time window
            }
        }
        
        // Limit the size of recent accesses
        if self.recent_accesses.len() > self.max_recent_accesses {
            self.recent_accesses.pop_front();
        }
        
        // Occasionally clean up old patterns (once every 10 minutes)
        let cleanup_interval = Duration::from_secs(600);
        if now.duration_since(self.last_cleanup) > cleanup_interval {
            self.cleanup_patterns();
            self.last_cleanup = now;
        }
    }
    
    /// Clean up access patterns that aren't significant
    fn cleanup_patterns(&mut self) {
        for (_, patterns) in self.co_access_patterns.iter_mut() {
            patterns.retain(|_, count| *count >= self.min_pattern_count);
        }
        
        // Remove entries with empty pattern maps
        self.co_access_patterns.retain(|_, patterns| !patterns.is_empty());
    }
    
    /// Get CIDs that are likely to be accessed soon after the given CID
    fn get_predicted_accesses(&self, cid: &Cid) -> Vec<Cid> {
        if let Some(patterns) = self.co_access_patterns.get(cid) {
            // Sort by count (frequency) in descending order
            let mut predictions: Vec<(Cid, usize)> = patterns
                .iter()
                .map(|(cid, count)| (*cid, *count))
                .collect();
            
            predictions.sort_by(|a, b| b.1.cmp(&a.1));
            
            // Return just the CIDs in order of likelihood
            predictions.into_iter().map(|(cid, _)| cid).collect()
        } else {
            Vec::new()
        }
    }
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
    
    /// Calculate the prefetch effectiveness (what percentage of prefetched nodes were used)
    pub fn prefetch_effectiveness(&self) -> f64 {
        if self.prefetched_nodes == 0 {
            0.0
        } else {
            (self.prefetch_hits as f64 / self.prefetched_nodes as f64) * 100.0
        }
    }
}

impl DagNodeCache {
    /// Create a new DAG node cache with the specified capacity
    pub fn new(capacity: usize) -> Self {
        Self::with_options(capacity, 2, 10, true)
    }
    
    /// Create a new DAG node cache with custom prefetch options
    pub fn with_options(
        capacity: usize,
        max_prefetch_depth: usize,
        max_prefetch_count: usize,
        prefetch_enabled: bool,
    ) -> Self {
        // Ensure capacity is at least 1
        let capacity = std::cmp::max(1, capacity);
        let cache = RwLock::new(LruCache::new(NonZeroUsize::new(capacity).unwrap()));
        let stats = Mutex::new(CacheStats::default());
        let prefetch_tracker = Mutex::new(HashSet::new());
        let access_patterns = Mutex::new(AccessPatternTracker::new(1000, 3));
        
        Self {
            cache,
            stats,
            prefetch_tracker,
            access_patterns,
            max_prefetch_depth,
            max_prefetch_count,
            prefetch_enabled,
        }
    }
    
    /// Get a node from the cache, if present
    pub fn get(&self, cid: &Cid) -> Option<Arc<DagNode>> {
        // Try to get from cache with a read lock first (fast path)
        let cache_read = self.cache.read().unwrap();
        let result = cache_read.peek(cid).cloned();
        
        // Update stats
        let mut stats = self.stats.lock().unwrap();
        if result.is_some() {
            stats.hits += 1;
            
            // If this was a prefetched node that was actually used, record that success
            if self.prefetch_tracker.lock().unwrap().remove(cid) {
                stats.prefetch_hits += 1;
            }
        } else {
            stats.misses += 1;
        }
        drop(stats);
        
        // Record this access for pattern tracking
        if let Ok(mut tracker) = self.access_patterns.lock() {
            tracker.record_access(cid);
        }
        
        result
    }
    
    /// Insert a node into the cache
    pub fn insert(&self, cid: Cid, node: Arc<DagNode>) {
        let mut cache = self.cache.write().unwrap();
        cache.put(cid, node);
        
        // Update stats
        let mut stats = self.stats.lock().unwrap();
        stats.insertions += 1;
    }
    
    /// Remove a node from the cache
    pub fn remove(&self, cid: &Cid) {
        let mut cache = self.cache.write().unwrap();
        cache.pop(cid);
        
        // Also remove from prefetch tracker if present
        let mut tracker = self.prefetch_tracker.lock().unwrap();
        tracker.remove(cid);
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
        
        // Clear the prefetch tracker as well
        let mut tracker = self.prefetch_tracker.lock().unwrap();
        tracker.clear();
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap();
        stats.clone()
    }
    
    /// Initiate predictive loading for all the parents and children linked to this node
    pub async fn predictive_load<F>(
        &self, 
        cid: &Cid, 
        node: &DagNode,
        load_node_fn: F
    ) where
        F: Fn(Cid) -> futures::future::BoxFuture<'static, Option<DagNode>> + Send + Sync + Clone + 'static,
    {
        if !self.prefetch_enabled {
            return;
        }
        
        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.predictive_loads += 1;
        }
        
        // First, try to load nodes based on access patterns
        self.load_predicted_nodes(cid, load_node_fn.clone()).await;
        
        // Then, load parent/child relationships if we haven't reached our quota
        self.load_related_nodes(cid, node, load_node_fn).await;
    }
    
    /// Load nodes based on predicted access patterns
    async fn load_predicted_nodes<F>(
        &self,
        cid: &Cid,
        load_node_fn: F
    ) where
        F: Fn(Cid) -> futures::future::BoxFuture<'static, Option<DagNode>> + Send + Sync + Clone + 'static,
    {
        // Get predicted CIDs based on access patterns
        let predicted_cids = {
            let tracker = self.access_patterns.lock().unwrap();
            tracker.get_predicted_accesses(cid)
        };
        
        if predicted_cids.is_empty() {
            return;
        }
        
        // Filter out CIDs already in cache or being prefetched
        let cids_to_load = {
            let cache_read = self.cache.read().unwrap();
            let mut tracker = self.prefetch_tracker.lock().unwrap();
            
            predicted_cids.into_iter()
                .filter(|predicted_cid| {
                    // Skip if already in cache
                    if cache_read.contains(predicted_cid) {
                        return false;
                    }
                    
                    // Skip if already being prefetched
                    if tracker.contains(predicted_cid) {
                        return false;
                    }
                    
                    // Mark as being prefetched and include in load list
                    tracker.insert(*predicted_cid);
                    true
                })
                .take(self.max_prefetch_count) // Limit number of prefetches
                .collect::<Vec<_>>()
        };
        
        if cids_to_load.is_empty() {
            return;
        }
        
        debug!("Predictively loading {} nodes based on access patterns", cids_to_load.len());
        
        // Start async loading of all predicted nodes
        let load_futures = cids_to_load.iter().map(|predicted_cid| {
            let load_fn = load_node_fn.clone();
            let cid_copy = *predicted_cid;
            let cache = self.clone();
            
            async move {
                if let Some(node) = load_fn(cid_copy).await {
                    // Insert into cache
                    cache.insert(cid_copy, Arc::new(node));
                    
                    // Update stats
                    let mut stats = cache.stats.lock().unwrap();
                    stats.prefetched_nodes += 1;
                    
                    Some((cid_copy, node))
                } else {
                    // Remove from tracker if load failed
                    let mut tracker = cache.prefetch_tracker.lock().unwrap();
                    tracker.remove(&cid_copy);
                    None
                }
            }
        });
        
        // Execute all loads concurrently
        let _results = join_all(load_futures).await;
    }
    
    /// Load directly related nodes (parents/links)
    async fn load_related_nodes<F>(
        &self,
        cid: &Cid,
        node: &DagNode,
        load_node_fn: F
    ) where
        F: Fn(Cid) -> futures::future::BoxFuture<'static, Option<DagNode>> + Send + Sync + Clone + 'static,
    {
        // Start with immediate parents
        let mut to_visit = VecDeque::new();
        let mut visited = HashSet::new();
        
        // Add direct parents to visit queue (depth 1)
        for parent_cid in &node.parents {
            to_visit.push_back((*parent_cid, 1));
            visited.insert(*parent_cid);
        }
        
        // Limit the number of nodes we'll prefetch
        let mut prefetch_count = 0;
        
        // BFS traversal up to max depth
        while let Some((next_cid, depth)) = to_visit.pop_front() {
            // Check if we've hit our prefetch limit
            if prefetch_count >= self.max_prefetch_count {
                break;
            }
            
            // Skip if already in cache
            {
                let cache_read = self.cache.read().unwrap();
                if cache_read.contains(&next_cid) {
                    continue;
                }
            }
            
            // Skip if already being prefetched
            {
                let mut tracker = self.prefetch_tracker.lock().unwrap();
                if tracker.contains(&next_cid) {
                    continue;
                }
                
                // Mark as being prefetched
                tracker.insert(next_cid);
            }
            
            // Load the node
            let load_fn = load_node_fn.clone();
            let cache = self.clone();
            let cid_copy = next_cid;
            
            let node_opt = load_fn(cid_copy).await;
            
            if let Some(loaded_node) = node_opt {
                // Insert into cache
                cache.insert(cid_copy, Arc::new(loaded_node.clone()));
                
                // Update stats
                {
                    let mut stats = cache.stats.lock().unwrap();
                    stats.prefetched_nodes += 1;
                }
                
                prefetch_count += 1;
                
                // If depth < max_depth, add this node's parents to the queue
                if depth < self.max_prefetch_depth {
                    for parent_cid in &loaded_node.parents {
                        if !visited.contains(parent_cid) {
                            to_visit.push_back((*parent_cid, depth + 1));
                            visited.insert(*parent_cid);
                        }
                    }
                }
            } else {
                // Remove from tracker if load failed
                let mut tracker = cache.prefetch_tracker.lock().unwrap();
                tracker.remove(&cid_copy);
            }
        }
        
        if prefetch_count > 0 {
            debug!("Predictively loaded {} related nodes", prefetch_count);
        }
    }
    
    /// Clone for use in async contexts
    pub fn clone(&self) -> Self {
        // We don't actually clone the underlying data, just the Arc references
        Self {
            cache: self.cache.clone(),
            stats: self.stats.clone(),
            prefetch_tracker: self.prefetch_tracker.clone(),
            access_patterns: self.access_patterns.clone(),
            max_prefetch_depth: self.max_prefetch_depth,
            max_prefetch_count: self.max_prefetch_count,
            prefetch_enabled: self.prefetch_enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::{DagNode, DagNodeMetadata, IdentityId};
    
    // Helper to create a mock DagNode for testing
    fn create_test_node(id: &str, parents: Vec<Cid>) -> DagNode {
        DagNode {
            payload: libipld::ipld!({ "id": id }),
            parents,
            issuer: IdentityId("did:icn:test".to_string()),
            signature: vec![1, 2, 3, 4],
            metadata: DagNodeMetadata::new(),
        }
    }
    
    // Get a test CID
    fn get_test_cid(id: &str) -> Cid {
        let mh = create_sha256_multihash(id.as_bytes());
        Cid::new_v1(0x71, mh)
    }
    
    #[tokio::test]
    async fn test_predictive_loading() {
        let cache = DagNodeCache::with_options(10, 2, 5, true);
        
        // Create a test DAG structure:
        // A -> B -> C
        //  \-> D
        
        let cid_a = get_test_cid("A");
        let cid_b = get_test_cid("B");
        let cid_c = get_test_cid("C");
        let cid_d = get_test_cid("D");
        
        let node_c = create_test_node("C", vec![]);
        let node_b = create_test_node("B", vec![cid_c]);
        let node_d = create_test_node("D", vec![]);
        let node_a = create_test_node("A", vec![cid_b, cid_d]);
        
        // Custom loader function that simulates loading nodes from storage
        let loader = move |cid: Cid| -> futures::future::BoxFuture<'static, Option<DagNode>> {
            let node_c = node_c.clone();
            let node_b = node_b.clone();
            let node_d = node_d.clone();
            let node_a = node_a.clone();
            
            Box::pin(async move {
                if cid == cid_a {
                    Some(node_a)
                } else if cid == cid_b {
                    Some(node_b)
                } else if cid == cid_c {
                    Some(node_c)
                } else if cid == cid_d {
                    Some(node_d)
                } else {
                    None
                }
            })
        };
        
        // Put A in the cache initially
        cache.insert(cid_a, Arc::new(node_a.clone()));
        
        // Trigger predictive loading from A
        cache.predictive_load(&cid_a, &node_a, loader).await;
        
        // Wait briefly for async operations to complete
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify that B and D (direct links) were loaded
        assert!(cache.get(&cid_b).is_some(), "Node B should be in cache after predictive loading");
        assert!(cache.get(&cid_d).is_some(), "Node D should be in cache after predictive loading");
        
        // Verify that C (second-level) was also loaded since max_depth is 2
        assert!(cache.get(&cid_c).is_some(), "Node C should be in cache after predictive loading");
        
        // Verify stats
        let stats = cache.stats();
        assert!(stats.prefetched_nodes >= 3, "At least 3 nodes should have been prefetched");
        assert_eq!(stats.prefetch_hits, 3, "All 3 prefetched nodes were accessed");
    }
    
    #[test]
    fn test_access_pattern_tracking() {
        let mut tracker = AccessPatternTracker::new(100, 2);
        
        let cid_a = get_test_cid("A");
        let cid_b = get_test_cid("B");
        let cid_c = get_test_cid("C");
        
        // Simulate a pattern: A -> B -> C (multiple times)
        for _ in 0..3 {
            tracker.record_access(&cid_a);
            std::thread::sleep(Duration::from_millis(100)); // Simulate time passing
            tracker.record_access(&cid_b);
            std::thread::sleep(Duration::from_millis(100));
            tracker.record_access(&cid_c);
            std::thread::sleep(Duration::from_millis(500)); // Separate the pattern instances
        }
        
        // Now A should predict B
        let predictions = tracker.get_predicted_accesses(&cid_a);
        assert!(!predictions.is_empty(), "Should have predicted nodes after A");
        assert_eq!(predictions[0], cid_b, "B should be the most likely node after A");
        
        // And B should predict C
        let predictions = tracker.get_predicted_accesses(&cid_b);
        assert!(!predictions.is_empty(), "Should have predicted nodes after B");
        assert_eq!(predictions[0], cid_c, "C should be the most likely node after B");
    }
} 