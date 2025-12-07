//! Peer synchronization state and backpressure management
//!
//! This module implements deficit-based backpressure for gossip synchronization:
//!
//! ## Deficit Accounting (Token Bucket Style)
//!
//! Each peer has a `deficit_bytes` counter that tracks send/receive balance:
//! - **Debit** (sending data): `deficit -= bytes_sent` (creates "debt")
//! - **Credit** (receiving data): `deficit += bytes_received * 2` (double credit rewards progress)
//! - **Backpressure**: If `deficit < -threshold`, pause pulls until recovery
//!
//! Example:
//! ```text
//! 1. Send 1000 bytes → deficit = -1000 (debt)
//! 2. Receive 500 bytes → deficit = -1000 + 1000 = 0 (double credit)
//! 3. If deficit < -10000, peer is backpressured → wait for backoff
//! ```
//!
//! ## Exponential Backoff with Jitter
//!
//! Retry timing doubles on each attempt until capped at max:
//! - Initial: 300ms (Partner) or 1500ms (Isolated)
//! - Growth: 300ms → 600ms → 1200ms (capped)
//! - Reset to minimum on successful response
//!
//! Trust classes get different backoff ranges:
//! - Isolated: 1500-5000ms (slow retry, prevent abuse)
//! - Known: 800-2500ms
//! - Partner/Federated: 300-1200ms (fast convergence)
//!
//! ## Trust-Gated Limits
//!
//! `can_send()` enforces three conditions:
//! 1. Not backpressured: `deficit > -threshold`
//! 2. Under outstanding request limit: `outstanding < max_outstanding_reqs`
//! 3. Backoff elapsed: `now - last_retry >= current_backoff`
//!
//! ## Usage
//!
//! ```rust,ignore
//! let mut manager = PeerSyncManager::new(300, 5000);
//! let peer_state = manager.get_or_create(peer_did);
//!
//! // Check if we can send
//! let limits = TrustResourceLimits::for_trust_class(trust_class);
//! if peer_state.can_send(limits.max_outstanding_reqs, 10000) {
//!     let nonce = peer_state.next_nonce();
//!     peer_state.record_request();
//!     // Send PullRequest...
//! }
//!
//! // On response received
//! peer_state.record_response(bytes_received);
//! ```

use crate::types::BloomFilterData;
use crate::vector_clock::VectorClock;
use icn_identity::Did;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Exponential backoff with jitter for retry timing
#[derive(Debug, Clone)]
pub struct Backoff {
    /// Current backoff duration
    current_ms: u64,

    /// Minimum backoff duration
    min_ms: u64,

    /// Maximum backoff duration
    max_ms: u64,

    /// Last retry time (if any)
    last_retry: Option<Instant>,
}

impl Backoff {
    /// Create a new backoff with min/max bounds
    pub fn new(min_ms: u64, max_ms: u64) -> Self {
        Self {
            current_ms: min_ms,
            min_ms,
            max_ms,
            last_retry: None,
        }
    }

    /// Check if we can retry now (backoff period has elapsed)
    pub fn can_retry(&self) -> bool {
        match self.last_retry {
            None => true,
            Some(last) => last.elapsed().as_millis() as u64 >= self.current_ms,
        }
    }

    /// Record a retry attempt and increase backoff
    pub fn record_retry(&mut self) {
        self.last_retry = Some(Instant::now());

        // Exponential increase with cap
        self.current_ms = (self.current_ms * 2).min(self.max_ms);
    }

    /// Reset backoff to minimum (on success)
    pub fn reset(&mut self) {
        self.current_ms = self.min_ms;
        self.last_retry = None;
    }

    /// Get current backoff duration
    pub fn current_duration(&self) -> Duration {
        Duration::from_millis(self.current_ms)
    }
}

/// Per-peer synchronization state for gossip protocol
#[derive(Debug, Clone)]
pub struct PeerSyncState {
    /// Peer DID
    pub peer_did: Did,

    /// Last known vector clock from this peer
    pub last_vector: VectorClock,

    /// Last known bloom filter from this peer
    pub last_bloom: Option<BloomFilterData>,

    /// Backpressure deficit in bytes (negative means debt, positive means credit)
    pub deficit_bytes: i64,

    /// Last nonce used for request correlation
    pub last_nonce: u64,

    /// Retry backoff state
    pub backoff: Backoff,

    /// Number of outstanding pull requests
    pub outstanding_requests: u32,

    /// Last sync attempt timestamp
    pub last_sync: Option<Instant>,
}

impl PeerSyncState {
    /// Create a new peer sync state
    pub fn new(peer_did: Did, min_backoff_ms: u64, max_backoff_ms: u64) -> Self {
        Self {
            peer_did,
            last_vector: VectorClock::new(),
            last_bloom: None,
            deficit_bytes: 0,
            last_nonce: 0,
            backoff: Backoff::new(min_backoff_ms, max_backoff_ms),
            outstanding_requests: 0,
            last_sync: None,
        }
    }

    /// Generate a new nonce for correlation
    pub fn next_nonce(&mut self) -> u64 {
        self.last_nonce = self.last_nonce.wrapping_add(1);
        self.last_nonce
    }

    /// Update vector clock from peer
    pub fn update_vector(&mut self, clock: VectorClock) {
        self.last_vector.merge(&clock);
    }

    /// Update bloom filter from peer
    pub fn update_bloom(&mut self, bloom: BloomFilterData) {
        self.last_bloom = Some(bloom);
    }

    /// Debit bytes (sending data to peer)
    pub fn debit_bytes(&mut self, bytes: u32) {
        self.deficit_bytes -= bytes as i64;
    }

    /// Credit bytes (successful sync or backoff recovery)
    pub fn credit_bytes(&mut self, bytes: u32) {
        self.deficit_bytes += bytes as i64;
    }

    /// Check if we're under backpressure (negative deficit beyond threshold)
    pub fn is_backpressured(&self, threshold: i64) -> bool {
        self.deficit_bytes < -threshold
    }

    /// Check if we can send more data (not backpressured and within request limits)
    pub fn can_send(&self, max_outstanding: u32, backpressure_threshold: i64) -> bool {
        !self.is_backpressured(backpressure_threshold)
            && self.outstanding_requests < max_outstanding
            && self.backoff.can_retry()
    }

    /// Record a pull request sent
    pub fn record_request(&mut self) {
        self.outstanding_requests += 1;
        self.backoff.record_retry();
        self.last_sync = Some(Instant::now());
    }

    /// Record a pull response received
    pub fn record_response(&mut self, bytes_received: u32) {
        if self.outstanding_requests > 0 {
            self.outstanding_requests -= 1;
        }

        // Credit received bytes (double credit to encourage progress)
        self.credit_bytes(bytes_received * 2);

        // Reset backoff on success
        self.backoff.reset();
    }
}

/// Manager for all peer sync states
#[derive(Debug)]
pub struct PeerSyncManager {
    /// Per-peer sync states
    states: HashMap<Did, PeerSyncState>,

    /// Default backoff settings (can be overridden per-peer based on trust)
    default_min_backoff_ms: u64,
    default_max_backoff_ms: u64,
}

impl PeerSyncManager {
    /// Create a new peer sync manager
    pub fn new(default_min_backoff_ms: u64, default_max_backoff_ms: u64) -> Self {
        Self {
            states: HashMap::new(),
            default_min_backoff_ms,
            default_max_backoff_ms,
        }
    }

    /// Get or create peer sync state
    pub fn get_or_create(&mut self, peer_did: Did) -> &mut PeerSyncState {
        self.states.entry(peer_did.clone()).or_insert_with(|| {
            PeerSyncState::new(
                peer_did,
                self.default_min_backoff_ms,
                self.default_max_backoff_ms,
            )
        })
    }

    /// Get peer sync state (read-only)
    pub fn get(&self, peer_did: &Did) -> Option<&PeerSyncState> {
        self.states.get(peer_did)
    }

    /// Get mutable peer sync state
    pub fn get_mut(&mut self, peer_did: &Did) -> Option<&mut PeerSyncState> {
        self.states.get_mut(peer_did)
    }

    /// Remove peer sync state
    pub fn remove(&mut self, peer_did: &Did) {
        self.states.remove(peer_did);
    }

    /// Get all peer DIDs
    pub fn peers(&self) -> Vec<Did> {
        self.states.keys().cloned().collect()
    }

    /// Get number of tracked peers
    pub fn peer_count(&self) -> usize {
        self.states.len()
    }

    /// Update backoff settings for a specific peer (based on trust class changes)
    pub fn update_peer_backoff(&mut self, peer_did: &Did, min_ms: u64, max_ms: u64) {
        if let Some(state) = self.states.get_mut(peer_did) {
            state.backoff = Backoff::new(min_ms, max_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_backoff_exponential_increase() {
        let mut backoff = Backoff::new(100, 1000);

        assert!(backoff.can_retry()); // Initially can retry

        backoff.record_retry();
        assert_eq!(backoff.current_ms, 200); // Doubled

        backoff.record_retry();
        assert_eq!(backoff.current_ms, 400);

        backoff.record_retry();
        assert_eq!(backoff.current_ms, 800);

        backoff.record_retry();
        assert_eq!(backoff.current_ms, 1000); // Capped at max

        backoff.reset();
        assert_eq!(backoff.current_ms, 100); // Reset to min
    }

    #[test]
    fn test_backoff_can_retry() {
        let mut backoff = Backoff::new(50, 1000);

        assert!(backoff.can_retry());

        backoff.record_retry();
        assert!(!backoff.can_retry()); // Too soon

        // Sleep to allow retry (with extra margin for slow CI)
        std::thread::sleep(Duration::from_millis(100));
        assert!(backoff.can_retry());
    }

    #[test]
    fn test_peer_sync_state_deficit() {
        let did = KeyPair::generate().unwrap().did().clone();
        let mut state = PeerSyncState::new(did, 100, 1000);

        assert_eq!(state.deficit_bytes, 0);

        // Debit (send data)
        state.debit_bytes(1000);
        assert_eq!(state.deficit_bytes, -1000);

        // Check backpressure
        assert!(state.is_backpressured(500)); // -1000 < -500
        assert!(!state.is_backpressured(1500)); // -1000 >= -1500

        // Credit (receive data)
        state.credit_bytes(500);
        assert_eq!(state.deficit_bytes, -500);
    }

    #[test]
    fn test_peer_sync_state_nonce() {
        let did = KeyPair::generate().unwrap().did().clone();
        let mut state = PeerSyncState::new(did, 100, 1000);

        assert_eq!(state.next_nonce(), 1);
        assert_eq!(state.next_nonce(), 2);
        assert_eq!(state.next_nonce(), 3);
    }

    #[test]
    fn test_peer_sync_state_can_send() {
        let did = KeyPair::generate().unwrap().did().clone();
        let mut state = PeerSyncState::new(did, 100, 1000);

        // Initially can send
        assert!(state.can_send(3, 1000));

        // Debit to trigger backpressure
        state.debit_bytes(2000);
        assert!(!state.can_send(3, 1000)); // Backpressured

        // Credit to recover
        state.credit_bytes(2500);
        assert!(state.can_send(3, 1000));

        // Max out outstanding requests
        state.outstanding_requests = 3;
        assert!(!state.can_send(3, 1000)); // At limit
    }

    #[test]
    fn test_peer_sync_manager() {
        let mut manager = PeerSyncManager::new(100, 1000);

        let did1 = KeyPair::generate().unwrap().did().clone();
        let did2 = KeyPair::generate().unwrap().did().clone();

        // Get or create
        let state1 = manager.get_or_create(did1.clone());
        state1.debit_bytes(500);

        let state2 = manager.get_or_create(did2.clone());
        state2.debit_bytes(1000);

        assert_eq!(manager.peer_count(), 2);

        // Check states
        assert_eq!(manager.get(&did1).unwrap().deficit_bytes, -500);
        assert_eq!(manager.get(&did2).unwrap().deficit_bytes, -1000);

        // Remove peer
        manager.remove(&did1);
        assert_eq!(manager.peer_count(), 1);
    }

    #[test]
    fn test_peer_sync_state_record_response() {
        let did = KeyPair::generate().unwrap().did().clone();
        let mut state = PeerSyncState::new(did, 100, 1000);

        // Send a request
        state.record_request();
        assert_eq!(state.outstanding_requests, 1);
        assert_eq!(state.backoff.current_ms, 200); // Backoff increased

        // Receive response
        state.record_response(1000);
        assert_eq!(state.outstanding_requests, 0);
        assert_eq!(state.deficit_bytes, 2000); // Double credit
        assert_eq!(state.backoff.current_ms, 100); // Backoff reset
    }
}
