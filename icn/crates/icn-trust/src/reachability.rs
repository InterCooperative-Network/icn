//! Reachability Bloom Filter for Trust Graph
//!
//! Provides O(1) negative lookups for trust path queries. When the bloom filter
//! says a DID is not reachable, we can immediately return 0.0 trust score
//! without any graph traversal.
//!
//! The filter tracks all DIDs that are reachable from our own DID via the
//! trust graph (direct or transitive within 2 hops).

use bloomfilter::Bloom;
use icn_identity::Did;
use std::sync::Mutex;

/// Bloom filter for fast "definitely not reachable" queries
///
/// This filter is populated with all DIDs that have a trust path from our node.
/// When checking trust for a DID:
/// - If filter says NO (definitely not present) → return 0.0 immediately
/// - If filter says MAYBE (possibly present) → do full computation
///
/// False positive rate: ~1% with 100k capacity
pub struct ReachabilityFilter {
    /// The bloom filter itself
    filter: Mutex<Bloom<String>>,
    /// Number of items inserted
    count: std::sync::atomic::AtomicUsize,
    /// Generation counter (incremented on rebuild)
    generation: std::sync::atomic::AtomicU64,
}

impl ReachabilityFilter {
    /// Expected items capacity
    const DEFAULT_CAPACITY: usize = 100_000;

    /// Target false positive rate (1%)
    const FALSE_POSITIVE_RATE: f64 = 0.01;

    /// Create a new reachability filter with default capacity
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// Create a new reachability filter with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let filter = Bloom::new_for_fp_rate(capacity, Self::FALSE_POSITIVE_RATE);
        Self {
            filter: Mutex::new(filter),
            count: std::sync::atomic::AtomicUsize::new(0),
            generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Check if a DID might be reachable (has trust path)
    ///
    /// Returns:
    /// - `false` = definitely NOT reachable, safe to return 0.0 immediately
    /// - `true` = possibly reachable, need to compute actual score
    #[inline]
    pub fn may_be_reachable(&self, did: &Did) -> bool {
        if let Ok(filter) = self.filter.lock() {
            filter.check(&did.to_string())
        } else {
            // If lock fails, assume reachable to avoid false negatives
            true
        }
    }

    /// Add a DID to the set of reachable targets
    pub fn add_reachable(&self, did: &Did) {
        if let Ok(mut filter) = self.filter.lock() {
            filter.set(&did.to_string());
            self.count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Rebuild the filter from a list of reachable DIDs
    ///
    /// This should be called when the trust graph topology changes significantly.
    pub fn rebuild(&self, reachable_dids: impl IntoIterator<Item = Did>) {
        let new_filter = Bloom::new_for_fp_rate(Self::DEFAULT_CAPACITY, Self::FALSE_POSITIVE_RATE);

        let mut count = 0;
        if let Ok(mut filter) = self.filter.lock() {
            *filter = new_filter;

            for did in reachable_dids {
                filter.set(&did.to_string());
                count += 1;
            }
        }

        self.count
            .store(count, std::sync::atomic::Ordering::Relaxed);
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        tracing::debug!(
            "Rebuilt reachability filter: {} DIDs, generation {}",
            count,
            self.generation.load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    /// Clear the filter
    pub fn clear(&self) {
        if let Ok(mut filter) = self.filter.lock() {
            *filter = Bloom::new_for_fp_rate(Self::DEFAULT_CAPACITY, Self::FALSE_POSITIVE_RATE);
        }
        self.count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get the number of items in the filter
    pub fn len(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Check if the filter is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the current generation (incremented on rebuild)
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for ReachabilityFilter {
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
    fn test_empty_filter_returns_false() {
        let filter = ReachabilityFilter::new();
        let did = make_test_did();

        // Empty filter should return false for any DID
        assert!(!filter.may_be_reachable(&did));
    }

    #[test]
    fn test_added_did_is_reachable() {
        let filter = ReachabilityFilter::new();
        let did = make_test_did();

        filter.add_reachable(&did);

        // Added DID should be reachable
        assert!(filter.may_be_reachable(&did));
        assert_eq!(filter.len(), 1);
    }

    #[test]
    fn test_rebuild_replaces_content() {
        let filter = ReachabilityFilter::new();
        let did1 = make_test_did();
        let did2 = make_test_did();
        let did3 = make_test_did();

        filter.add_reachable(&did1);
        assert!(filter.may_be_reachable(&did1));

        // Rebuild with different DIDs
        filter.rebuild([did2.clone(), did3.clone()]);

        // did1 should no longer be present (may have false positive)
        // did2 and did3 should be present
        assert!(filter.may_be_reachable(&did2));
        assert!(filter.may_be_reachable(&did3));
        assert_eq!(filter.len(), 2);
        assert_eq!(filter.generation(), 1);
    }

    #[test]
    fn test_clear() {
        let filter = ReachabilityFilter::new();
        let did = make_test_did();

        filter.add_reachable(&did);
        assert_eq!(filter.len(), 1);

        filter.clear();
        assert!(filter.is_empty());
    }

    #[test]
    fn test_false_positive_rate() {
        // This test verifies the bloom filter doesn't have too many false positives
        let filter = ReachabilityFilter::new();

        // Add 1000 DIDs
        let added: Vec<Did> = (0..1000).map(|_| make_test_did()).collect();
        for did in &added {
            filter.add_reachable(did);
        }

        // Check 1000 DIDs that were NOT added
        let not_added: Vec<Did> = (0..1000).map(|_| make_test_did()).collect();
        let false_positives = not_added
            .iter()
            .filter(|did| filter.may_be_reachable(did))
            .count();

        // With 1% false positive rate, expect ~10 false positives
        // Allow up to 50 for test stability
        assert!(
            false_positives < 50,
            "Too many false positives: {}",
            false_positives
        );
    }
}
