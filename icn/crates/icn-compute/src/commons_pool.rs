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

/// Sybil resistance policy for commons pool participation.
///
/// Two-tier trust threshold:
/// - **Affiliated nodes** (with `cell_id`) are held to `min_trust_score`
///   (default 0.1). Org membership implies accountability; a higher bar
///   prevents low-quality nodes from diluting cell-endorsed capacity.
/// - **Unaffiliated nodes** (no `cell_id`) are admitted under
///   `commons_admission_min_trust` (default 0.0). The commons is
///   open-participation by design. Trust score still influences scheduling
///   priority — it just does not gate admission for unaffiliated nodes.
///
/// `max_participants` caps the pool for both paths (spam guard).
#[derive(Debug, Clone)]
pub struct SybilPolicy {
    /// Minimum trust score for **affiliated** nodes (with cell_id) to join.
    /// Default: 0.1. Nodes below this threshold are rejected.
    pub min_trust_score: f64,
    /// Minimum trust score for **unaffiliated** nodes (no cell_id) to join.
    /// Default: 0.0 — any node may attempt commons participation.
    /// Set higher on curated networks that require prior trust relationships.
    pub commons_admission_min_trust: f64,
    /// Maximum number of participants in the pool.
    /// Prevents unbounded growth from sybil flooding.
    pub max_participants: usize,
}

impl Default for SybilPolicy {
    fn default() -> Self {
        Self {
            min_trust_score: 0.1,
            commons_admission_min_trust: 0.0,
            max_participants: 10_000,
        }
    }
}

/// Error returned when a participant fails sybil resistance checks.
#[derive(Debug, Clone, PartialEq)]
pub enum SybilRejection {
    /// Trust score is below the minimum threshold.
    InsufficientTrust { score: f64, required: f64 },
    /// Pool is at maximum capacity.
    PoolFull { capacity: usize },
}

impl std::fmt::Display for SybilRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SybilRejection::InsufficientTrust { score, required } => {
                write!(
                    f,
                    "insufficient trust score: {score:.2} < {required:.2} required"
                )
            }
            SybilRejection::PoolFull { capacity } => {
                write!(f, "commons pool full: {capacity} participants at capacity")
            }
        }
    }
}

impl std::error::Error for SybilRejection {}

/// Configuration for stale participant expiry.
///
/// Participants whose `last_announce` is older than `max_age` are removed
/// from the pool on the next [`CommonsPool::expire_stale`] call.
///
/// The default (300 seconds) matches a typical keep-alive interval of 60–120
/// seconds with a 2–3× safety margin.
#[derive(Debug, Clone)]
pub struct StaleParticipantConfig {
    /// Maximum age of a participant's last announce before they are evicted.
    pub max_age: std::time::Duration,
}

impl Default for StaleParticipantConfig {
    fn default() -> Self {
        Self {
            max_age: std::time::Duration::from_secs(300),
        }
    }
}

/// A node participating in the commons resource pool.
#[derive(Debug, Clone)]
pub struct CommonsParticipant {
    /// DID of the participating node.
    pub did: String,
    /// Reported capacity of the node.
    pub capacity: NodeCapacity,
    /// Capacity budget controlling how much is shared with each scope.
    pub budget: CapacityBudget,
    /// Trust score of this participant at the time of admission (0.0–1.0).
    /// Used for sybil resistance: participants below the policy threshold
    /// are rejected.
    pub trust_score: f64,
    /// Whether this node has a cell affiliation (cell_id was Some on announce).
    ///
    /// Affiliated nodes are held to `SybilPolicy::min_trust_score`.
    /// Unaffiliated nodes are admitted under the lower
    /// `SybilPolicy::commons_admission_min_trust` threshold.
    pub is_affiliated: bool,
    /// When we last heard from this node. In-memory only — not serialized,
    /// not sent over the wire. Used by [`CommonsPool::expire_stale`] to
    /// remove participants that have gone silent.
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
    sybil_policy: SybilPolicy,
}

impl CommonsPool {
    /// Create an empty commons pool with default sybil policy.
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
            sybil_policy: SybilPolicy::default(),
        }
    }

    /// Create a commons pool with a custom sybil policy.
    pub fn with_policy(policy: SybilPolicy) -> Self {
        Self {
            participants: HashMap::new(),
            sybil_policy: policy,
        }
    }

    /// Get the current sybil policy.
    pub fn sybil_policy(&self) -> &SybilPolicy {
        &self.sybil_policy
    }

    /// Add or update a participant in the pool.
    ///
    /// **Note**: This method bypasses sybil checks. Prefer [`try_add_participant`]
    /// for admission from untrusted sources (e.g., gossip announcements).
    pub fn add_participant(&mut self, participant: CommonsParticipant) {
        self.participants
            .insert(participant.did.clone(), participant);
    }

    /// Add a participant after validating against the sybil policy.
    ///
    /// Returns `Err(SybilRejection)` if the participant fails checks:
    /// - Trust score below the applicable threshold (both new and existing):
    ///   - Affiliated nodes: `min_trust_score` (default 0.1)
    ///   - Unaffiliated nodes: `commons_admission_min_trust` (default 0.0)
    /// - Pool at `max_participants` capacity (new participants only)
    ///
    /// Existing participants whose trust drops below threshold are
    /// **rejected** (returns `InsufficientTrust`). The caller is
    /// responsible for evicting them from the pool if desired — this
    /// method does not remove them automatically.
    pub fn try_add_participant(
        &mut self,
        participant: CommonsParticipant,
    ) -> Result<(), SybilRejection> {
        let is_existing = self.participants.contains_key(&participant.did);

        // Select the trust threshold based on affiliation.
        // Affiliated nodes (with a cell_id) are held to the higher bar.
        // Unaffiliated nodes use the lower commons admission threshold so
        // that a new node with zero trust-graph history can still participate.
        let min_trust = if participant.is_affiliated {
            self.sybil_policy.min_trust_score
        } else {
            self.sybil_policy.commons_admission_min_trust
        };

        // Trust score check applies to both new and existing participants.
        // A participant whose trust drops below their threshold is evicted on
        // next announce rather than silently remaining.
        if participant.trust_score < min_trust {
            return Err(SybilRejection::InsufficientTrust {
                score: participant.trust_score,
                required: min_trust,
            });
        }

        // Capacity check only applies to new participants.
        if !is_existing && self.participants.len() >= self.sybil_policy.max_participants {
            return Err(SybilRejection::PoolFull {
                capacity: self.sybil_policy.max_participants,
            });
        }

        self.participants
            .insert(participant.did.clone(), participant);
        Ok(())
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

    /// Remove participants that have not re-announced within `max_age`.
    ///
    /// Returns the DIDs of all removed participants. The caller should emit
    /// a metric and log entry proportional to the count.
    ///
    /// Designed to be called inside `on_capacity_announce()` on each incoming
    /// announce — piggybacking on the announce cadence avoids a background
    /// timer for V1. Callers with more precise timing requirements may call
    /// this on their own schedule.
    ///
    /// Idempotent: calling with `max_age = Duration::MAX` removes nothing.
    pub fn expire_stale(&mut self, max_age: std::time::Duration) -> Vec<String> {
        let stale: Vec<String> = self
            .participants
            .iter()
            .filter(|(_, p)| p.last_announce.elapsed() > max_age)
            .map(|(did, _)| did.clone())
            .collect();
        for did in &stale {
            self.participants.remove(did);
        }
        stale
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
            trust_score: 0.5,
            is_affiliated: false,
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

    fn make_participant_with_trust(did: &str, trust_score: f64) -> CommonsParticipant {
        CommonsParticipant {
            did: did.to_string(),
            capacity: make_capacity(4.0, 8192, 50000),
            budget: CapacityBudget {
                commons_share: 0.5,
                ..CapacityBudget::default()
            },
            trust_score,
            is_affiliated: true,
            last_announce: Instant::now(),
        }
    }

    #[test]
    fn test_sybil_rejects_low_trust() {
        let policy = SybilPolicy {
            min_trust_score: 0.2,
            commons_admission_min_trust: 0.0,
            max_participants: 100,
        };
        let mut pool = CommonsPool::with_policy(policy);

        // Trust score 0.1 < threshold 0.2 → rejected
        let result = pool.try_add_participant(make_participant_with_trust("did:icn:low", 0.1));
        assert_eq!(
            result,
            Err(SybilRejection::InsufficientTrust {
                score: 0.1,
                required: 0.2,
            })
        );
        assert_eq!(pool.participant_count(), 0);
    }

    #[test]
    fn test_sybil_accepts_sufficient_trust() {
        let policy = SybilPolicy {
            min_trust_score: 0.2,
            commons_admission_min_trust: 0.0,
            max_participants: 100,
        };
        let mut pool = CommonsPool::with_policy(policy);

        let result = pool.try_add_participant(make_participant_with_trust("did:icn:good", 0.3));
        assert!(result.is_ok());
        assert_eq!(pool.participant_count(), 1);
    }

    #[test]
    fn test_sybil_rejects_pool_full() {
        let policy = SybilPolicy {
            min_trust_score: 0.0,
            commons_admission_min_trust: 0.0,
            max_participants: 2,
        };
        let mut pool = CommonsPool::with_policy(policy);

        pool.try_add_participant(make_participant_with_trust("did:icn:a", 0.5))
            .unwrap();
        pool.try_add_participant(make_participant_with_trust("did:icn:b", 0.5))
            .unwrap();

        // Third participant rejected — pool full
        let result = pool.try_add_participant(make_participant_with_trust("did:icn:c", 0.5));
        assert_eq!(result, Err(SybilRejection::PoolFull { capacity: 2 }));
    }

    #[test]
    fn test_sybil_allows_existing_participant_update() {
        let policy = SybilPolicy {
            min_trust_score: 0.0,
            commons_admission_min_trust: 0.0,
            max_participants: 1,
        };
        let mut pool = CommonsPool::with_policy(policy);

        pool.try_add_participant(make_participant_with_trust("did:icn:a", 0.5))
            .unwrap();

        // Same DID can update even though pool is "full"
        let result = pool.try_add_participant(make_participant_with_trust("did:icn:a", 0.6));
        assert!(result.is_ok());
        assert_eq!(pool.participant_count(), 1);
    }

    #[test]
    fn test_unaffiliated_zero_trust_admitted() {
        // Acceptance criterion #1 (#947): unaffiliated node with 0.0 trust joins
        // the commons pool under default SybilPolicy (commons_admission_min_trust = 0.0).
        let mut pool = CommonsPool::new();
        let participant = CommonsParticipant {
            did: "did:icn:newcomer".to_string(),
            capacity: make_capacity(4.0, 8192, 50000),
            budget: CapacityBudget {
                commons_share: 1.0,
                ..CapacityBudget::default()
            },
            trust_score: 0.0,
            is_affiliated: false,
            last_announce: Instant::now(),
        };
        assert!(pool.try_add_participant(participant).is_ok());
        assert!(pool.contains("did:icn:newcomer"));
    }

    #[test]
    fn test_affiliated_zero_trust_rejected() {
        // Acceptance criterion #4 (#947): affiliated node with 0.0 trust is still
        // rejected by the default affiliated threshold (min_trust_score = 0.1).
        let mut pool = CommonsPool::new();
        let participant = CommonsParticipant {
            did: "did:icn:affiliated-newcomer".to_string(),
            capacity: make_capacity(4.0, 8192, 50000),
            budget: CapacityBudget {
                commons_share: 0.1,
                ..CapacityBudget::default()
            },
            trust_score: 0.0,
            is_affiliated: true,
            last_announce: Instant::now(),
        };
        assert_eq!(
            pool.try_add_participant(participant),
            Err(SybilRejection::InsufficientTrust {
                score: 0.0,
                required: 0.1,
            })
        );
        assert!(!pool.contains("did:icn:affiliated-newcomer"));
    }

    // ─── expire_stale tests (#964) ───────────────────────────────────────────

    /// Build a participant whose last_announce is `age_secs` seconds in the past.
    fn make_stale_participant(did: &str, age_secs: u64) -> CommonsParticipant {
        CommonsParticipant {
            did: did.to_string(),
            capacity: make_capacity(4.0, 8192, 50000),
            budget: CapacityBudget {
                commons_share: 1.0,
                ..CapacityBudget::default()
            },
            trust_score: 0.5,
            is_affiliated: false,
            last_announce: Instant::now() - std::time::Duration::from_secs(age_secs),
        }
    }

    #[test]
    fn test_expire_stale_removes_old_participants() {
        // Participants older than max_age are evicted; recent ones are retained.
        let mut pool = CommonsPool::new();
        pool.add_participant(make_stale_participant("did:icn:old", 400));
        pool.add_participant(make_participant("did:icn:recent", 4.0, 8192, 50000, 1.0));

        let expired = pool.expire_stale(std::time::Duration::from_secs(300));

        assert_eq!(expired, vec!["did:icn:old".to_string()]);
        assert_eq!(pool.participant_count(), 1);
        assert!(!pool.contains("did:icn:old"));
        assert!(pool.contains("did:icn:recent"));
    }

    #[test]
    fn test_expire_stale_retains_all_when_none_stale() {
        // When all participants announced recently, expire_stale removes nothing.
        let mut pool = CommonsPool::new();
        pool.add_participant(make_participant("did:icn:a", 4.0, 8192, 50000, 1.0));
        pool.add_participant(make_participant("did:icn:b", 4.0, 8192, 50000, 1.0));

        let expired = pool.expire_stale(std::time::Duration::from_secs(300));

        assert!(expired.is_empty());
        assert_eq!(pool.participant_count(), 2);
    }

    #[test]
    fn test_expire_stale_duration_max_removes_nothing() {
        // Duration::MAX is the "disabled" sentinel — idempotent, no evictions.
        let mut pool = CommonsPool::new();
        pool.add_participant(make_stale_participant("did:icn:ancient", 999_999));

        let expired = pool.expire_stale(std::time::Duration::MAX);

        assert!(expired.is_empty());
        assert_eq!(pool.participant_count(), 1);
    }

    #[test]
    fn test_sybil_evicts_on_trust_drop() {
        let policy = SybilPolicy {
            min_trust_score: 0.2,
            commons_admission_min_trust: 0.0,
            max_participants: 100,
        };
        let mut pool = CommonsPool::with_policy(policy);

        // Initially admitted
        pool.try_add_participant(make_participant_with_trust("did:icn:a", 0.5))
            .unwrap();
        assert_eq!(pool.participant_count(), 1);

        // Trust drops below threshold on re-announce → rejected
        let result = pool.try_add_participant(make_participant_with_trust("did:icn:a", 0.1));
        assert_eq!(
            result,
            Err(SybilRejection::InsufficientTrust {
                score: 0.1,
                required: 0.2,
            })
        );
        // Pool does NOT auto-evict — it returns the error and the caller
        // decides whether to evict. In production, placement.rs calls
        // `pool.remove_participant()` on InsufficientTrust rejection.
        assert_eq!(pool.participant_count(), 1);
    }
}
