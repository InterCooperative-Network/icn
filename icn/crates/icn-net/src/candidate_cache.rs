//! Connection candidate cache for NAT traversal
//!
//! Stores received connection candidates from peers and provides TTL-based
//! expiration to prevent stale connection attempts.

use crate::ConnectionCandidate;
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default TTL for cached candidates (5 minutes)
pub const DEFAULT_CANDIDATE_TTL_SECS: u64 = 300;

/// Cache for storing connection candidates from peers
pub struct CandidateCache {
    /// Cached candidates by DID
    candidates: Arc<RwLock<HashMap<Did, ConnectionCandidate>>>,

    /// TTL for candidates in seconds
    ttl_secs: u64,
}

impl CandidateCache {
    /// Create a new candidate cache with default TTL
    pub fn new() -> Self {
        Self::with_ttl(DEFAULT_CANDIDATE_TTL_SECS)
    }

    /// Create a new candidate cache with custom TTL
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            candidates: Arc::new(RwLock::new(HashMap::new())),
            ttl_secs,
        }
    }

    /// Store a candidate in the cache
    ///
    /// Returns true if this is a new or updated candidate, false if ignored (stale)
    pub async fn store(&self, candidate: ConnectionCandidate) -> bool {
        // Check freshness before storing
        if !candidate.is_fresh(self.ttl_secs) {
            debug!(
                "Ignoring stale candidate from {} (age: {}s, max: {}s)",
                candidate.did,
                candidate.age_secs(),
                self.ttl_secs
            );
            return false;
        }

        let did = candidate.did.clone();
        let mut cache = self.candidates.write().await;

        // Check if we already have a candidate for this DID
        let is_new_or_updated = match cache.get(&did) {
            Some(existing) => {
                // Only update if the new candidate is fresher
                if candidate.timestamp > existing.timestamp {
                    info!(
                        "Updating candidate for {} (age: {}s)",
                        did,
                        candidate.age_secs()
                    );
                    true
                } else {
                    debug!(
                        "Ignoring older candidate from {} (existing: {}, new: {})",
                        did, existing.timestamp, candidate.timestamp
                    );
                    false
                }
            }
            None => {
                info!(
                    "Storing new candidate for {} (local={}, public={:?})",
                    did, candidate.local_addr, candidate.public_addr
                );
                true
            }
        };

        if is_new_or_updated {
            cache.insert(did, candidate);
        }

        is_new_or_updated
    }

    /// Get a candidate for a specific DID
    ///
    /// Returns None if no candidate exists or if the cached candidate is stale
    pub async fn get(&self, did: &Did) -> Option<ConnectionCandidate> {
        let cache = self.candidates.read().await;
        let candidate = cache.get(did)?;

        // Check if still fresh
        if candidate.is_fresh(self.ttl_secs) {
            Some(candidate.clone())
        } else {
            debug!(
                "Cached candidate for {} is stale (age: {}s)",
                did,
                candidate.age_secs()
            );
            None
        }
    }

    /// Remove a candidate from the cache
    pub async fn remove(&self, did: &Did) {
        self.candidates.write().await.remove(did);
    }

    /// Clean up expired candidates
    ///
    /// Returns the number of candidates removed
    pub async fn cleanup_expired(&self) -> usize {
        let mut cache = self.candidates.write().await;
        let initial_size = cache.len();

        // Remove stale entries
        cache.retain(|did, candidate| {
            let is_fresh = candidate.is_fresh(self.ttl_secs);
            if !is_fresh {
                debug!(
                    "Removing expired candidate for {} (age: {}s)",
                    did,
                    candidate.age_secs()
                );
            }
            is_fresh
        });

        let removed = initial_size - cache.len();
        if removed > 0 {
            info!("Cleaned up {} expired candidates", removed);
        }
        removed
    }

    /// Get the number of cached candidates
    pub async fn len(&self) -> usize {
        self.candidates.read().await.len()
    }

    /// Check if cache is empty
    pub async fn is_empty(&self) -> bool {
        self.candidates.read().await.is_empty()
    }
}

impl Default for CandidateCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_candidate_cache_store_and_get() {
        let cache = CandidateCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let candidate = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5000".parse().unwrap(),
            Some("203.0.113.5:5000".parse().unwrap()),
            None,
        );

        // Store candidate
        assert!(cache.store(candidate.clone()).await);

        // Retrieve candidate
        let retrieved = cache.get(&did).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().did, did);

        // Cache size
        assert_eq!(cache.len().await, 1);
        assert!(!cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_candidate_cache_stale_rejected() {
        let cache = CandidateCache::with_ttl(60); // 1 minute TTL
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut candidate = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5000".parse().unwrap(),
            None,
            None,
        );

        // Make candidate stale (older than 1 minute)
        candidate.timestamp -= 120; // 2 minutes ago

        // Store should reject stale candidate
        assert!(!cache.store(candidate).await);
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_candidate_cache_update_fresher() {
        let cache = CandidateCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Store first candidate
        let candidate1 = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5000".parse().unwrap(),
            None,
            None,
        );
        assert!(cache.store(candidate1.clone()).await);

        // Store fresher candidate (manually adjust timestamp to ensure it's newer)
        let mut candidate2 = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5001".parse().unwrap(),
            Some("203.0.113.5:5001".parse().unwrap()),
            None,
        );
        candidate2.timestamp = candidate1.timestamp + 10; // 10 seconds newer

        assert!(cache.store(candidate2.clone()).await);

        // Should have the newer candidate
        let retrieved = cache.get(&did).await.unwrap();
        assert_eq!(retrieved.local_addr, candidate2.local_addr);
        assert_eq!(cache.len().await, 1); // Still just one entry
    }

    #[tokio::test]
    async fn test_candidate_cache_ignore_older() {
        let cache = CandidateCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Store candidate
        let candidate1 = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5000".parse().unwrap(),
            None,
            None,
        );
        assert!(cache.store(candidate1.clone()).await);

        // Try to store older candidate
        let mut older_candidate = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5001".parse().unwrap(),
            None,
            None,
        );
        older_candidate.timestamp = candidate1.timestamp - 10; // 10 seconds older

        assert!(!cache.store(older_candidate).await);

        // Should still have the original candidate
        let retrieved = cache.get(&did).await.unwrap();
        assert_eq!(retrieved.local_addr, candidate1.local_addr);
    }

    #[tokio::test]
    async fn test_candidate_cache_cleanup() {
        let cache = CandidateCache::with_ttl(60); // 1 minute TTL
        let keypair1 = KeyPair::generate().unwrap();
        let keypair2 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();
        let did2 = keypair2.did().clone();

        // Store fresh candidate
        let fresh = ConnectionCandidate::new(
            did1.clone(),
            "192.168.1.100:5000".parse().unwrap(),
            None,
            None,
        );
        cache.store(fresh).await;

        // Store stale candidate
        let mut stale = ConnectionCandidate::new(
            did2.clone(),
            "192.168.1.101:5000".parse().unwrap(),
            None,
            None,
        );
        stale.timestamp -= 120; // 2 minutes ago (stale)
                                // Force store by modifying cache directly (for test)
        cache.candidates.write().await.insert(did2.clone(), stale);

        assert_eq!(cache.len().await, 2);

        // Cleanup should remove stale candidate
        let removed = cache.cleanup_expired().await;
        assert_eq!(removed, 1);
        assert_eq!(cache.len().await, 1);

        // Only fresh candidate should remain
        assert!(cache.get(&did1).await.is_some());
        assert!(cache.get(&did2).await.is_none());
    }

    #[tokio::test]
    async fn test_candidate_cache_remove() {
        let cache = CandidateCache::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let candidate = ConnectionCandidate::new(
            did.clone(),
            "192.168.1.100:5000".parse().unwrap(),
            None,
            None,
        );
        cache.store(candidate).await;

        assert_eq!(cache.len().await, 1);

        // Remove candidate
        cache.remove(&did).await;
        assert!(cache.is_empty().await);
        assert!(cache.get(&did).await.is_none());
    }
}
