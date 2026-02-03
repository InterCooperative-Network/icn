//! Trust Graph Cache (Phase 19 Week 1)
//!
//! LRU cache for trust score lookups with TTL-based invalidation.
//! Reduces trust computation from O(n²) to O(1) for cached entries.

use icn_identity::Did;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Trust score with timestamp
#[derive(Debug, Clone)]
pub struct CachedScore {
    /// The cached trust score
    pub score: f64,
    /// When this score was computed
    pub computed_at: Instant,
}

impl CachedScore {
    /// Check if this cached score is still valid (within TTL)
    pub fn is_valid(&self, ttl: Duration) -> bool {
        self.computed_at.elapsed() < ttl
    }
}

/// LRU cache for trust scores with TTL-based invalidation
///
/// Features:
/// - LRU eviction when capacity exceeded
/// - TTL-based invalidation (default: 5 minutes)
/// - Thread-safe access via interior mutability
/// - Configurable maximum entries (default: 1000)
///
/// Performance: O(1) lookups for cached entries, O(n²) cache misses
pub struct TrustCache {
    /// LRU cache mapping DID to (score, timestamp)
    cache: Mutex<LruCache<Did, CachedScore>>,

    /// Time-to-live for cached entries
    ttl: Duration,

    /// Maximum number of cached entries
    max_entries: usize,
}

impl TrustCache {
    /// Create a new trust cache with default settings
    ///
    /// Defaults:
    /// - max_entries: 1000
    /// - ttl: 5 minutes
    pub fn new() -> Self {
        Self::with_config(1000, Duration::from_secs(300))
    }

    /// Create a new trust cache with custom configuration
    pub fn with_config(max_entries: usize, ttl: Duration) -> Self {
        // SAFETY: 1 is always non-zero, and max_entries defaults to 1 if zero
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            cache: Mutex::new(LruCache::new(capacity)),
            ttl,
            max_entries,
        }
    }

    /// Get a trust score from cache if valid
    ///
    /// Returns None if:
    /// - Entry not in cache
    /// - Entry expired (past TTL)
    pub fn get(&self, did: &Did) -> Option<f64> {
        let mut cache = self.cache.lock().ok()?;

        if let Some(cached) = cache.get(did) {
            if cached.is_valid(self.ttl) {
                icn_obs::metrics::scalability::trust_cache_hits_inc();
                return Some(cached.score);
            } else {
                // Expired - remove it
                cache.pop(did);
                icn_obs::metrics::scalability::trust_cache_expired_inc();
            }
        }

        icn_obs::metrics::scalability::trust_cache_misses_inc();
        None
    }

    /// Insert or update a trust score in the cache
    pub fn put(&self, did: Did, score: f64) {
        if let Ok(mut cache) = self.cache.lock() {
            let cached = CachedScore {
                score,
                computed_at: Instant::now(),
            };
            cache.put(did, cached);
            icn_obs::metrics::scalability::trust_cache_size_set(cache.len());
        }
    }

    /// Invalidate a specific DID (e.g., when trust edge changes)
    pub fn invalidate(&self, did: &Did) {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.pop(did).is_some() {
                icn_obs::metrics::scalability::trust_cache_invalidations_inc();
                icn_obs::metrics::scalability::trust_cache_size_set(cache.len());
            }
        }
    }

    /// Selective invalidation: only invalidate if entry exists in cache.
    ///
    /// Returns true if an entry was invalidated, false if skipped.
    /// This optimization reduces lock contention and metric noise for high-fanout
    /// scenarios where most downstream nodes have no cached scores.
    pub fn invalidate_if_cached(&self, did: &Did) -> bool {
        match self.cache.lock() {
            Ok(mut cache) => {
                // Use pop() directly for atomic check-and-remove
                if cache.pop(did).is_some() {
                    icn_obs::metrics::scalability::trust_cache_invalidations_inc();
                    true
                } else {
                    icn_obs::metrics::scalability::trust_cache_selective_skips_inc();
                    false
                }
            }
            Err(e) => {
                tracing::warn!("Cache lock poisoned during invalidation of {did}: {e}");
                icn_obs::metrics::scalability::trust_cache_lock_failures_inc();
                false
            }
        }
    }
    
    /// Get the current cache size (for batched metric updates).
    pub fn size(&self) -> usize {
        self.cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Clear all cached entries
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
            icn_obs::metrics::scalability::trust_cache_size_set(0);
        }
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get cache capacity
    pub fn capacity(&self) -> usize {
        self.max_entries
    }

    /// Get TTL duration
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Warm cache with pre-computed scores
    ///
    /// Useful for batch loading trust scores at startup
    pub fn warm(&self, scores: impl IntoIterator<Item = (Did, f64)>) {
        if let Ok(mut cache) = self.cache.lock() {
            let now = Instant::now();
            for (did, score) in scores {
                cache.put(
                    did,
                    CachedScore {
                        score,
                        computed_at: now,
                    },
                );
            }
            icn_obs::metrics::scalability::trust_cache_size_set(cache.len());
        }
    }

    /// Prune expired entries (manual cleanup)
    ///
    /// Note: Expired entries are automatically removed on access,
    /// but this can be called periodically for proactive cleanup.
    pub fn prune_expired(&self) -> usize {
        let mut pruned = 0;
        if let Ok(mut cache) = self.cache.lock() {
            // Collect expired keys
            let expired: Vec<Did> = cache
                .iter()
                .filter(|(_, cached)| !cached.is_valid(self.ttl))
                .map(|(did, _)| did.clone())
                .collect();

            // Remove expired entries
            for did in expired {
                cache.pop(&did);
                pruned += 1;
            }

            if pruned > 0 {
                icn_obs::metrics::scalability::trust_cache_size_set(cache.len());
            }
        }
        pruned
    }
}

impl Default for TrustCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_cache_basic() {
        let cache = TrustCache::new();
        let did = make_test_did();

        // Cache miss
        assert_eq!(cache.get(&did), None);

        // Insert
        cache.put(did.clone(), 0.75);

        // Cache hit
        assert_eq!(cache.get(&did), Some(0.75));
    }

    #[test]
    fn test_cache_update() {
        let cache = TrustCache::new();
        let did = make_test_did();

        cache.put(did.clone(), 0.5);
        assert_eq!(cache.get(&did), Some(0.5));

        // Update score
        cache.put(did.clone(), 0.8);
        assert_eq!(cache.get(&did), Some(0.8));
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = TrustCache::new();
        let did = make_test_did();

        cache.put(did.clone(), 0.6);
        assert_eq!(cache.get(&did), Some(0.6));

        // Invalidate
        cache.invalidate(&did);
        assert_eq!(cache.get(&did), None);
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let cache = TrustCache::with_config(1000, Duration::from_millis(50));
        let did = make_test_did();

        cache.put(did.clone(), 0.7);
        assert_eq!(cache.get(&did), Some(0.7));

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(60));

        // Should be expired
        assert_eq!(cache.get(&did), None);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = TrustCache::with_config(2, Duration::from_secs(300));
        let did1 = make_test_did();
        let did2 = make_test_did();
        let did3 = make_test_did();

        // Fill cache to capacity
        cache.put(did1.clone(), 0.1);
        cache.put(did2.clone(), 0.2);
        assert_eq!(cache.len(), 2);

        // Access did1 to make it recently used
        assert_eq!(cache.get(&did1), Some(0.1));

        // Insert did3 - should evict did2 (least recently used)
        cache.put(did3.clone(), 0.3);
        assert_eq!(cache.len(), 2);

        // did1 and did3 should be in cache, did2 should be evicted
        assert_eq!(cache.get(&did1), Some(0.1));
        assert_eq!(cache.get(&did3), Some(0.3));
        assert_eq!(cache.get(&did2), None);
    }

    #[test]
    fn test_cache_clear() {
        let cache = TrustCache::new();
        let did1 = make_test_did();
        let did2 = make_test_did();

        cache.put(did1.clone(), 0.5);
        cache.put(did2.clone(), 0.6);
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&did1), None);
        assert_eq!(cache.get(&did2), None);
    }

    #[test]
    fn test_cache_warm() {
        let cache = TrustCache::new();
        let did1 = make_test_did();
        let did2 = make_test_did();

        let scores = vec![(did1.clone(), 0.4), (did2.clone(), 0.8)];
        cache.warm(scores);

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&did1), Some(0.4));
        assert_eq!(cache.get(&did2), Some(0.8));
    }

    #[test]
    fn test_cache_prune_expired() {
        let cache = TrustCache::with_config(1000, Duration::from_millis(50));
        let did1 = make_test_did();
        let did2 = make_test_did();

        cache.put(did1.clone(), 0.5);
        std::thread::sleep(Duration::from_millis(60));
        cache.put(did2.clone(), 0.6); // Fresh entry

        // Prune expired
        let pruned = cache.prune_expired();
        assert_eq!(pruned, 1);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&did1), None);
        assert_eq!(cache.get(&did2), Some(0.6));
    }

    #[test]
    fn test_cache_capacity() {
        let cache = TrustCache::with_config(100, Duration::from_secs(300));
        assert_eq!(cache.capacity(), 100);
        assert_eq!(cache.ttl(), Duration::from_secs(300));
    }

    #[test]
    fn test_cache_is_empty() {
        let cache = TrustCache::new();
        assert!(cache.is_empty());

        let did = make_test_did();
        cache.put(did, 0.5);
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_selective_invalidation() {
        let cache = TrustCache::new();
        let did1 = make_test_did();
        let did2 = make_test_did();

        // Put one entry in cache
        cache.put(did1.clone(), 0.5);

        // Selective invalidation on cached entry should return true
        assert!(cache.invalidate_if_cached(&did1));
        assert_eq!(cache.get(&did1), None);

        // Selective invalidation on non-cached entry should return false
        assert!(!cache.invalidate_if_cached(&did2));
    }

    #[test]
    fn test_selective_invalidation_skip_count() {
        let cache = TrustCache::new();
        let did1 = make_test_did();
        let did2 = make_test_did();
        let did3 = make_test_did();

        // Cache only did1 and did2
        cache.put(did1.clone(), 0.5);
        cache.put(did2.clone(), 0.6);

        // Invalidate cached entries
        assert!(cache.invalidate_if_cached(&did1));
        assert!(cache.invalidate_if_cached(&did2));

        // Invalidate non-cached entry (should be skipped)
        assert!(!cache.invalidate_if_cached(&did3));

        // Verify cache is now empty
        assert!(cache.is_empty());
    }
}
