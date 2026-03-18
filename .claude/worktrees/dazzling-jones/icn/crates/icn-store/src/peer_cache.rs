//! Peer cache for persisting discovered peers
//!
//! This module provides persistent storage for discovered peers, enabling
//! faster network rejoining after restarts. Peers are stored with their
//! source (bootstrap, peer_exchange, mdns) and automatically expire.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, SystemTime};

/// Default TTL for cached peers (24 hours)
pub const DEFAULT_PEER_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Storage key prefix for peer cache entries
const PEER_CACHE_PREFIX: &[u8] = b"peer_cache:";

/// Source of peer discovery
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PeerSource {
    /// Configured bootstrap peer
    Bootstrap,
    /// Discovered via peer exchange protocol
    PeerExchange,
    /// Discovered via mDNS (local network)
    Mdns,
    /// Discovered via gossip candidate announcement
    CandidateAnnounce,
}

impl std::fmt::Display for PeerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerSource::Bootstrap => write!(f, "bootstrap"),
            PeerSource::PeerExchange => write!(f, "peer_exchange"),
            PeerSource::Mdns => write!(f, "mdns"),
            PeerSource::CandidateAnnounce => write!(f, "candidate_announce"),
        }
    }
}

/// Cached peer entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPeer {
    /// Peer's DID
    pub did: String,
    /// Primary address (local or public)
    pub address: SocketAddr,
    /// Optional public address (if different from primary)
    pub public_address: Option<SocketAddr>,
    /// How this peer was discovered
    pub source: PeerSource,
    /// When this peer was first seen
    pub first_seen: SystemTime,
    /// When this peer was last successfully connected
    pub last_connected: SystemTime,
    /// Number of successful connections
    pub connection_count: u32,
    /// Number of failed connection attempts since last success
    pub failure_count: u32,
}

impl CachedPeer {
    /// Create a new cached peer entry
    pub fn new(did: String, address: SocketAddr, source: PeerSource) -> Self {
        let now = SystemTime::now();
        Self {
            did,
            address,
            public_address: None,
            source,
            first_seen: now,
            last_connected: now,
            connection_count: 1,
            failure_count: 0,
        }
    }

    /// Check if this peer entry has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        match self.last_connected.elapsed() {
            Ok(elapsed) => elapsed > ttl,
            Err(_) => false, // Clock went backwards, assume not expired
        }
    }

    /// Record a successful connection
    pub fn record_success(&mut self) {
        self.last_connected = SystemTime::now();
        self.connection_count = self.connection_count.saturating_add(1);
        self.failure_count = 0;
    }

    /// Record a failed connection attempt
    pub fn record_failure(&mut self) {
        self.failure_count = self.failure_count.saturating_add(1);
    }

    /// Get a reliability score (0.0 to 1.0)
    ///
    /// Higher score = more reliable peer (more successes, fewer recent failures)
    pub fn reliability_score(&self) -> f64 {
        if self.connection_count == 0 {
            return 0.0;
        }

        // Base score from success ratio
        let success_ratio =
            self.connection_count as f64 / (self.connection_count + self.failure_count) as f64;

        // Penalty for recent failures (exponential decay)
        let failure_penalty = 0.9_f64.powi(self.failure_count as i32);

        success_ratio * failure_penalty
    }
}

/// Peer cache manager for storing and retrieving cached peers
pub struct PeerCache<'a, S: crate::Store> {
    store: &'a S,
    ttl: Duration,
}

impl<'a, S: crate::Store> PeerCache<'a, S> {
    /// Create a new peer cache with the given store and TTL
    pub fn new(store: &'a S, ttl: Duration) -> Self {
        Self { store, ttl }
    }

    /// Create a peer cache with default TTL (24 hours)
    pub fn with_default_ttl(store: &'a S) -> Self {
        Self::new(store, DEFAULT_PEER_TTL)
    }

    /// Store or update a cached peer
    pub fn put(&self, peer: &CachedPeer) -> Result<()> {
        let key = Self::make_key(&peer.did);
        let value = serde_json::to_vec(peer).context("Failed to serialize cached peer")?;
        self.store.put(&key, &value)
    }

    /// Get a cached peer by DID
    pub fn get(&self, did: &str) -> Result<Option<CachedPeer>> {
        let key = Self::make_key(did);
        match self.store.get(&key)? {
            Some(value) => {
                let peer: CachedPeer =
                    serde_json::from_slice(&value).context("Failed to deserialize cached peer")?;

                // Check if expired
                if peer.is_expired(self.ttl) {
                    // Remove expired entry
                    let _ = self.store.delete(&key);
                    Ok(None)
                } else {
                    Ok(Some(peer))
                }
            }
            None => Ok(None),
        }
    }

    /// Remove a cached peer
    pub fn remove(&self, did: &str) -> Result<()> {
        let key = Self::make_key(did);
        self.store.delete(&key)
    }

    /// Get all cached peers, filtering out expired entries
    pub fn get_all(&self) -> Result<Vec<CachedPeer>> {
        let entries = self.store.scan(PEER_CACHE_PREFIX)?;
        let mut peers = Vec::with_capacity(entries.len());
        let mut expired_keys = Vec::new();

        for (key, value) in entries {
            match serde_json::from_slice::<CachedPeer>(&value) {
                Ok(peer) => {
                    if peer.is_expired(self.ttl) {
                        expired_keys.push(key);
                    } else {
                        peers.push(peer);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to deserialize cached peer: {}", e);
                    expired_keys.push(key);
                }
            }
        }

        // Clean up expired entries
        for key in expired_keys {
            let _ = self.store.delete(&key);
        }

        Ok(peers)
    }

    /// Get cached peers sorted by reliability score (best first)
    pub fn get_best_peers(&self, limit: usize) -> Result<Vec<CachedPeer>> {
        let mut peers = self.get_all()?;
        peers.sort_by(|a, b| {
            b.reliability_score()
                .partial_cmp(&a.reliability_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        peers.truncate(limit);
        Ok(peers)
    }

    /// Get cached peers by source
    pub fn get_by_source(&self, source: PeerSource) -> Result<Vec<CachedPeer>> {
        let peers = self.get_all()?;
        Ok(peers.into_iter().filter(|p| p.source == source).collect())
    }

    /// Record a successful connection to a peer
    pub fn record_success(&self, did: &str, address: SocketAddr, source: PeerSource) -> Result<()> {
        let peer = match self.get(did)? {
            Some(mut existing) => {
                existing.record_success();
                existing.address = address; // Update to latest address
                existing
            }
            None => {
                // New peer - CachedPeer::new already sets connection_count to 1
                CachedPeer::new(did.to_string(), address, source)
            }
        };
        self.put(&peer)
    }

    /// Record a failed connection attempt
    pub fn record_failure(&self, did: &str) -> Result<()> {
        if let Some(mut peer) = self.get(did)? {
            peer.record_failure();
            self.put(&peer)?;
        }
        Ok(())
    }

    /// Get count of cached peers
    pub fn count(&self) -> Result<usize> {
        self.store.scan_count(PEER_CACHE_PREFIX)
    }

    /// Clear all cached peers
    pub fn clear(&self) -> Result<()> {
        let entries = self.store.scan(PEER_CACHE_PREFIX)?;
        for (key, _) in entries {
            self.store.delete(&key)?;
        }
        Ok(())
    }

    /// Generate storage key for a peer DID
    fn make_key(did: &str) -> Vec<u8> {
        let mut key = Vec::with_capacity(PEER_CACHE_PREFIX.len() + did.len());
        key.extend_from_slice(PEER_CACHE_PREFIX);
        key.extend_from_slice(did.as_bytes());
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SledStore;

    fn test_store() -> SledStore {
        SledStore::temporary().unwrap()
    }

    #[test]
    fn test_cached_peer_new() {
        let peer = CachedPeer::new(
            "did:icn:test".to_string(),
            "127.0.0.1:7777".parse().unwrap(),
            PeerSource::Bootstrap,
        );

        assert_eq!(peer.did, "did:icn:test");
        assert_eq!(peer.connection_count, 1);
        assert_eq!(peer.failure_count, 0);
        assert_eq!(peer.source, PeerSource::Bootstrap);
    }

    #[test]
    fn test_cached_peer_expiry() {
        let mut peer = CachedPeer::new(
            "did:icn:test".to_string(),
            "127.0.0.1:7777".parse().unwrap(),
            PeerSource::Bootstrap,
        );

        // Not expired with default TTL
        assert!(!peer.is_expired(DEFAULT_PEER_TTL));

        // Simulate old peer
        peer.last_connected = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
        assert!(peer.is_expired(DEFAULT_PEER_TTL));
    }

    #[test]
    fn test_cached_peer_reliability_score() {
        let mut peer = CachedPeer::new(
            "did:icn:test".to_string(),
            "127.0.0.1:7777".parse().unwrap(),
            PeerSource::Bootstrap,
        );

        // Initial score should be high (1 success, 0 failures)
        assert!(peer.reliability_score() > 0.9);

        // Record some successes
        peer.record_success();
        peer.record_success();
        assert!(peer.reliability_score() > 0.9);

        // Record failures
        peer.record_failure();
        peer.record_failure();
        let score_with_failures = peer.reliability_score();
        assert!(score_with_failures < 0.9);

        // Success resets failure count
        peer.record_success();
        assert!(peer.reliability_score() > score_with_failures);
    }

    #[test]
    fn test_peer_cache_put_get() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        let peer = CachedPeer::new(
            "did:icn:peer1".to_string(),
            "192.168.1.100:7777".parse().unwrap(),
            PeerSource::PeerExchange,
        );

        cache.put(&peer).unwrap();

        let retrieved = cache.get("did:icn:peer1").unwrap().unwrap();
        assert_eq!(retrieved.did, "did:icn:peer1");
        assert_eq!(retrieved.source, PeerSource::PeerExchange);
    }

    #[test]
    fn test_peer_cache_get_all() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        // Add multiple peers
        for i in 0..5 {
            let peer = CachedPeer::new(
                format!("did:icn:peer{i}"),
                format!("192.168.1.{i}:7777").parse().unwrap(),
                PeerSource::PeerExchange,
            );
            cache.put(&peer).unwrap();
        }

        let all = cache.get_all().unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_peer_cache_get_best_peers() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        // Add peers with different reliability
        let mut peer1 = CachedPeer::new(
            "did:icn:reliable".to_string(),
            "192.168.1.1:7777".parse().unwrap(),
            PeerSource::Bootstrap,
        );
        peer1.connection_count = 100;
        peer1.failure_count = 0;

        let mut peer2 = CachedPeer::new(
            "did:icn:unreliable".to_string(),
            "192.168.1.2:7777".parse().unwrap(),
            PeerSource::PeerExchange,
        );
        peer2.connection_count = 10;
        peer2.failure_count = 5;

        cache.put(&peer1).unwrap();
        cache.put(&peer2).unwrap();

        let best = cache.get_best_peers(2).unwrap();
        assert_eq!(best.len(), 2);
        assert_eq!(best[0].did, "did:icn:reliable"); // Most reliable first
    }

    #[test]
    fn test_peer_cache_record_success_failure() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        // Record success creates entry if not exists
        cache
            .record_success(
                "did:icn:new",
                "192.168.1.1:7777".parse().unwrap(),
                PeerSource::Bootstrap,
            )
            .unwrap();

        let peer = cache.get("did:icn:new").unwrap().unwrap();
        assert_eq!(peer.connection_count, 1);

        // Record more successes
        cache
            .record_success(
                "did:icn:new",
                "192.168.1.1:7777".parse().unwrap(),
                PeerSource::Bootstrap,
            )
            .unwrap();

        let peer = cache.get("did:icn:new").unwrap().unwrap();
        assert_eq!(peer.connection_count, 2);

        // Record failure
        cache.record_failure("did:icn:new").unwrap();
        let peer = cache.get("did:icn:new").unwrap().unwrap();
        assert_eq!(peer.failure_count, 1);
    }

    #[test]
    fn test_peer_cache_remove() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        let peer = CachedPeer::new(
            "did:icn:removeme".to_string(),
            "192.168.1.1:7777".parse().unwrap(),
            PeerSource::Mdns,
        );
        cache.put(&peer).unwrap();

        assert!(cache.get("did:icn:removeme").unwrap().is_some());

        cache.remove("did:icn:removeme").unwrap();
        assert!(cache.get("did:icn:removeme").unwrap().is_none());
    }

    #[test]
    fn test_peer_source_display() {
        assert_eq!(PeerSource::Bootstrap.to_string(), "bootstrap");
        assert_eq!(PeerSource::PeerExchange.to_string(), "peer_exchange");
        assert_eq!(PeerSource::Mdns.to_string(), "mdns");
        assert_eq!(
            PeerSource::CandidateAnnounce.to_string(),
            "candidate_announce"
        );
    }

    #[test]
    fn test_peer_cache_count() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        assert_eq!(cache.count().unwrap(), 0);

        for i in 0..3 {
            let peer = CachedPeer::new(
                format!("did:icn:peer{i}"),
                format!("192.168.1.{i}:7777").parse().unwrap(),
                PeerSource::Bootstrap,
            );
            cache.put(&peer).unwrap();
        }

        assert_eq!(cache.count().unwrap(), 3);
    }

    #[test]
    fn test_peer_cache_clear() {
        let store = test_store();
        let cache = PeerCache::with_default_ttl(&store);

        for i in 0..5 {
            let peer = CachedPeer::new(
                format!("did:icn:peer{i}"),
                format!("192.168.1.{i}:7777").parse().unwrap(),
                PeerSource::Bootstrap,
            );
            cache.put(&peer).unwrap();
        }

        assert_eq!(cache.count().unwrap(), 5);

        cache.clear().unwrap();
        assert_eq!(cache.count().unwrap(), 0);
    }
}
