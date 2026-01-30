//! Commons resource pool tracking.
//!
//! **Architectural invariant**: `CommonsPool` is advisory scheduling state.
//! It is ephemeral, rebuilt from gossip announcements. The ledger is
//! authoritative economic state — this pool must never mutate economic truth.
//!
//! **Architectural invariant**: Affiliated nodes without an explicit budget
//! default to `CapacityBudget::default()` (0.10 commons share), not full
//! commons. Omitting a budget must never accidentally opt a node fully into
//! commons — that is a governance footgun.

use std::collections::HashMap;
use std::time::Instant;

use crate::scheduler::{CapacityBudget, NodeCapacity};

/// A node participating in the commons resource pool.
#[derive(Debug, Clone)]
pub struct CommonsParticipant {
    /// DID of the participating node.
    pub did: String,
    /// Reported capacity of the node.
    pub capacity: NodeCapacity,
    /// Capacity budget controlling how much is shared with each scope.
    pub budget: CapacityBudget,
    /// When we last heard from this node. In-memory only — not serialized,
    /// not sent over the wire. Enables future stale-participant expiry
    /// without migration.
    // TODO(#925): Implement stale participant expiry using last_announce.
    // Nodes not announcing for >5 minutes should be removed from the pool.
    pub last_announce: Instant,
}

/// Aggregate capacity across all commons participants.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateCapacity {
    /// Total commons-weighted CPU cores.
    pub cpu_cores: f64,
    /// Total commons-weighted memory in MB.
    pub memory_mb: u64,
    /// Total commons-weighted storage in MB.
    pub storage_mb: u64,
    /// Number of participating nodes.
    pub node_count: usize,
}

/// Advisory pool of nodes contributing to the commons.
///
/// **This is advisory scheduling state.** Ephemeral, rebuilt from gossip.
/// The ledger is authoritative economic state.
///
/// **Lock discipline**: Never hold a write lock on `CommonsPool` during
/// heavy computation. Acquire, mutate, release.
#[derive(Debug)]
pub struct CommonsPool {
    participants: HashMap<String, CommonsParticipant>,
}

impl CommonsPool {
    /// Create an empty commons pool.
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
        }
    }

    /// Add or update a participant in the pool.
    pub fn add_participant(&mut self, participant: CommonsParticipant) {
        self.participants
            .insert(participant.did.clone(), participant);
    }

    /// Remove a participant by DID. Returns the removed participant if present.
    pub fn remove_participant(&mut self, did: &str) -> Option<CommonsParticipant> {
        self.participants.remove(did)
    }

    /// Number of participants currently in the pool.
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Check whether a DID is in the pool.
    pub fn contains(&self, did: &str) -> bool {
        self.participants.contains_key(did)
    }

    /// Get a participant by DID.
    pub fn get_participant(&self, did: &str) -> Option<&CommonsParticipant> {
        self.participants.get(did)
    }

    /// Iterate over all participants.
    pub fn participants(&self) -> impl Iterator<Item = &CommonsParticipant> {
        self.participants.values()
    }

    /// Compute the total commons-weighted capacity across all participants.
    ///
    /// Each node's capacity is weighted by its `budget.commons_share`.
    ///
    /// # Thread Safety
    ///
    /// This method takes `&self` and iterates over the participants map.
    /// Callers are responsible for ensuring exclusive access — in practice
    /// this is always called while holding an `RwLock` (read or write)
    /// on the `CommonsPool`.
    pub fn total_commons_capacity(&self) -> AggregateCapacity {
        let mut cpu_cores: f64 = 0.0;
        let mut memory_mb: u128 = 0;
        let mut storage_mb: u128 = 0;

        for p in self.participants.values() {
            let share = p.budget.commons_share;
            cpu_cores += p.capacity.cpu_cores_available * share;
            memory_mb += (p.capacity.memory_mb_available as f64 * share) as u128;
            storage_mb += (p.capacity.storage_mb_available as f64 * share) as u128;
        }

        AggregateCapacity {
            cpu_cores,
            // Saturate u128 → u64: values above u64::MAX are clamped.
            memory_mb: memory_mb.min(u64::MAX as u128) as u64,
            storage_mb: storage_mb.min(u64::MAX as u128) as u64,
            node_count: self.participants.len(),
        }
    }
}

impl Default for CommonsPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_capacity(cpu: f64, mem: u64, storage: u64) -> NodeCapacity {
        NodeCapacity {
            cpu_cores_total: cpu,
            cpu_cores_available: cpu,
            memory_mb_total: mem,
            memory_mb_available: mem,
            storage_mb_available: storage,
            network_mbps: 100.0,
            gpu_devices: vec![],
            updated_at: 1000,
        }
    }

    fn make_participant(
        did: &str,
        cpu: f64,
        mem: u64,
        storage: u64,
        share: f64,
    ) -> CommonsParticipant {
        CommonsParticipant {
            did: did.to_string(),
            capacity: make_capacity(cpu, mem, storage),
            budget: CapacityBudget {
                commons_share: share,
                ..CapacityBudget::default()
            },
            last_announce: Instant::now(),
        }
    }

    #[test]
    fn test_add_remove_participant() {
        let mut pool = CommonsPool::new();
        assert_eq!(pool.participant_count(), 0);
        assert!(!pool.contains("did:icn:alice"));

        pool.add_participant(make_participant("did:icn:alice", 4.0, 8192, 50000, 0.5));
        assert_eq!(pool.participant_count(), 1);
        assert!(pool.contains("did:icn:alice"));

        let removed = pool.remove_participant("did:icn:alice");
        assert!(removed.is_some());
        assert_eq!(pool.participant_count(), 0);
        assert!(!pool.contains("did:icn:alice"));
    }

    #[test]
    fn test_weighted_aggregation() {
        let mut pool = CommonsPool::new();

        // Node A: 4 cores, 8GB, 50GB — 50% commons share
        pool.add_participant(make_participant("did:icn:a", 4.0, 8192, 50000, 0.5));
        // Node B: 8 cores, 16GB, 100GB — 100% commons share (unaffiliated)
        pool.add_participant(make_participant("did:icn:b", 8.0, 16384, 100000, 1.0));

        let agg = pool.total_commons_capacity();
        assert_eq!(agg.node_count, 2);
        // A contributes 4*0.5=2.0 cores, B contributes 8*1.0=8.0 cores → 10.0
        assert!((agg.cpu_cores - 10.0).abs() < f64::EPSILON);
        // A: 8192*0.5=4096, B: 16384*1.0=16384 → 20480
        assert_eq!(agg.memory_mb, 20480);
        // A: 50000*0.5=25000, B: 100000*1.0=100000 → 125000
        assert_eq!(agg.storage_mb, 125000);
    }

    #[test]
    fn test_empty_pool() {
        let pool = CommonsPool::new();
        let agg = pool.total_commons_capacity();
        assert_eq!(agg.node_count, 0);
        assert!((agg.cpu_cores).abs() < f64::EPSILON);
        assert_eq!(agg.memory_mb, 0);
        assert_eq!(agg.storage_mb, 0);
    }

    #[test]
    fn test_contains_and_get() {
        let mut pool = CommonsPool::new();
        pool.add_participant(make_participant("did:icn:alice", 4.0, 8192, 50000, 0.5));

        assert!(pool.contains("did:icn:alice"));
        assert!(!pool.contains("did:icn:bob"));

        let p = pool.get_participant("did:icn:alice");
        assert!(p.is_some());
        assert_eq!(p.unwrap().did, "did:icn:alice");

        assert!(pool.get_participant("did:icn:bob").is_none());
    }
}
