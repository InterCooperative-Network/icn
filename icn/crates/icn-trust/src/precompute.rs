//! Background Precomputation for Trust Graph
//!
//! Tracks frequently-accessed DIDs and precomputes their trust scores
//! in the background to keep the cache warm.

use icn_identity::Did;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

/// Tracks access frequency and enables background precomputation
///
/// This component:
/// 1. Records which DIDs are frequently queried
/// 2. Identifies the "hot" DIDs (top N most accessed)
/// 3. Provides a list of DIDs to precompute during idle time
///
/// # Example
/// ```ignore
/// let mut precomputer = TrustPrecomputer::new(100);
///
/// // Record accesses
/// precomputer.record_access(&did);
///
/// // Get hot DIDs for precomputation
/// let hot = precomputer.get_hot_dids();
/// for did in hot {
///     let _ = graph.compute_trust_score(&did);
/// }
/// ```
pub struct TrustPrecomputer {
    /// Access frequency counter (DID -> access count)
    access_counts: Mutex<LruCache<Did, u64>>,
    /// Number of top DIDs to track
    top_n: usize,
    /// Default precomputation interval
    interval: Duration,
}

impl TrustPrecomputer {
    /// Default number of hot DIDs to track
    const DEFAULT_TOP_N: usize = 100;

    /// Default capacity for access tracking
    const DEFAULT_CAPACITY: usize = 10_000;

    /// Default precomputation interval
    const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);

    /// Create a new precomputer with default settings
    pub fn new() -> Self {
        Self::with_config(Self::DEFAULT_TOP_N, Self::DEFAULT_INTERVAL)
    }

    /// Create a new precomputer tracking top N DIDs
    pub fn with_top_n(top_n: usize) -> Self {
        Self::with_config(top_n, Self::DEFAULT_INTERVAL)
    }

    /// Create a new precomputer with custom configuration
    pub fn with_config(top_n: usize, interval: Duration) -> Self {
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(Self::DEFAULT_CAPACITY).unwrap();
        Self {
            access_counts: Mutex::new(LruCache::new(capacity)),
            top_n,
            interval,
        }
    }

    /// Record an access to a DID (call this on each lookup)
    pub fn record_access(&self, did: &Did) {
        if let Ok(mut counts) = self.access_counts.lock() {
            let count = counts.get(did).copied().unwrap_or(0);
            counts.put(did.clone(), count + 1);
        }
    }

    /// Get the top N most frequently accessed DIDs
    ///
    /// Returns DIDs sorted by access count (highest first)
    pub fn get_hot_dids(&self) -> Vec<Did> {
        let mut entries: Vec<(Did, u64)> = Vec::new();

        if let Ok(counts) = self.access_counts.lock() {
            for (did, count) in counts.iter() {
                entries.push((did.clone(), *count));
            }
        }

        // Sort by count (descending)
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        // Take top N
        entries
            .into_iter()
            .take(self.top_n)
            .map(|(did, _)| did)
            .collect()
    }

    /// Get the access count for a specific DID
    pub fn get_access_count(&self, did: &Did) -> u64 {
        self.access_counts
            .lock()
            .ok()
            .and_then(|mut counts| counts.get(did).copied())
            .unwrap_or(0)
    }

    /// Reset all access counts
    pub fn reset(&self) {
        if let Ok(mut counts) = self.access_counts.lock() {
            counts.clear();
        }
    }

    /// Get the configured precomputation interval
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Get the configured top N
    pub fn top_n(&self) -> usize {
        self.top_n
    }

    /// Get current number of tracked DIDs
    pub fn tracked_count(&self) -> usize {
        self.access_counts.lock().map(|c| c.len()).unwrap_or(0)
    }
}

impl Default for TrustPrecomputer {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to run precomputation with a trust graph
///
/// This is meant to be called periodically (e.g., every minute)
/// to warm the cache for frequently-accessed DIDs.
pub fn precompute_hot_dids<F>(precomputer: &TrustPrecomputer, mut compute_fn: F)
where
    F: FnMut(&Did),
{
    let hot_dids = precomputer.get_hot_dids();
    tracing::debug!("Precomputing trust scores for {} hot DIDs", hot_dids.len());

    for did in hot_dids {
        compute_fn(&did);
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
    fn test_record_access() {
        let precomputer = TrustPrecomputer::new();
        let did = make_test_did();

        assert_eq!(precomputer.get_access_count(&did), 0);

        precomputer.record_access(&did);
        assert_eq!(precomputer.get_access_count(&did), 1);

        precomputer.record_access(&did);
        assert_eq!(precomputer.get_access_count(&did), 2);
    }

    #[test]
    fn test_get_hot_dids() {
        let precomputer = TrustPrecomputer::with_top_n(3);
        let dids: Vec<Did> = (0..10).map(|_| make_test_did()).collect();

        // Access some DIDs different numbers of times
        for _ in 0..10 {
            precomputer.record_access(&dids[0]); // 10 times
        }
        for _ in 0..5 {
            precomputer.record_access(&dids[1]); // 5 times
        }
        for _ in 0..3 {
            precomputer.record_access(&dids[2]); // 3 times
        }
        precomputer.record_access(&dids[3]); // 1 time

        let hot = precomputer.get_hot_dids();

        assert_eq!(hot.len(), 3);
        assert_eq!(hot[0], dids[0]); // Most accessed
        assert_eq!(hot[1], dids[1]); // Second most
        assert_eq!(hot[2], dids[2]); // Third most
    }

    #[test]
    fn test_reset() {
        let precomputer = TrustPrecomputer::new();
        let did = make_test_did();

        precomputer.record_access(&did);
        assert_eq!(precomputer.tracked_count(), 1);

        precomputer.reset();
        assert_eq!(precomputer.tracked_count(), 0);
        assert_eq!(precomputer.get_access_count(&did), 0);
    }

    #[test]
    fn test_precompute_hot_dids() {
        let precomputer = TrustPrecomputer::with_top_n(2);
        let dids: Vec<Did> = (0..5).map(|_| make_test_did()).collect();

        precomputer.record_access(&dids[0]);
        precomputer.record_access(&dids[0]);
        precomputer.record_access(&dids[1]);

        let mut computed: Vec<Did> = Vec::new();
        precompute_hot_dids(&precomputer, |did| {
            computed.push(did.clone());
        });

        assert_eq!(computed.len(), 2);
    }

    #[test]
    fn test_configuration() {
        let precomputer = TrustPrecomputer::with_config(50, Duration::from_secs(120));

        assert_eq!(precomputer.top_n(), 50);
        assert_eq!(precomputer.interval(), Duration::from_secs(120));
    }
}
