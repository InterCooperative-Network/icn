//! Migration policy for actor placement decisions.
//!
//! Policies determine WHEN and WHERE to migrate actors based on:
//! - Executor load (load balancing)
//! - Data locality (Phase 16C integration)
//! - Network topology (RTT, bandwidth)
//! - Cooperative rules (governance integration)

use crate::actor_model::{ActorId, ActorRuntimeState, MigrationDecision, MigrationReason};
use crate::scheduler::NodeCapacity;
use std::collections::HashMap;

/// Information about available executors for migration decisions.
#[derive(Debug, Clone)]
pub struct ExecutorInfo {
    /// Executor DID
    pub did: String,
    /// Trust score (0.0 - 1.0)
    pub trust_score: f64,
    /// Current load (0.0 - 1.0, where 1.0 = fully utilized)
    pub load: f64,
    /// Current capacity
    pub capacity: NodeCapacity,
    /// Number of tasks in queue
    pub queue_depth: usize,
    /// Last seen timestamp (Unix milliseconds)
    pub last_seen: u64,
}

/// Network state snapshot for migration decisions.
#[derive(Debug, Clone)]
pub struct NetworkState {
    /// Current executor's load (0.0 - 1.0)
    pub executor_load: f64,

    /// Available executors with capacity
    pub available_executors: Vec<ExecutorInfo>,

    /// RTT to other executors (DID -> milliseconds)
    pub executor_rtt: HashMap<String, u64>,

    /// Data blob locations (hash -> list of executor DIDs)
    pub blob_locations: HashMap<[u8; 32], Vec<String>>,

    /// When this snapshot was taken
    pub snapshot_at: u64,
}

impl NetworkState {
    /// Find executors with capacity below threshold
    pub fn executors_under_load(&self, max_load: f64) -> Vec<&ExecutorInfo> {
        self.available_executors
            .iter()
            .filter(|exec| exec.load < max_load)
            .collect()
    }

    /// Find executor with most required blobs locally
    pub fn executor_with_most_blobs(
        &self,
        required_blobs: &[[u8; 32]],
        exclude: Option<&str>,
    ) -> Option<(String, usize)> {
        let mut blob_counts: HashMap<String, usize> = HashMap::new();

        for blob_hash in required_blobs {
            if let Some(locations) = self.blob_locations.get(blob_hash) {
                for did in locations {
                    if let Some(exclude_did) = exclude {
                        if did == exclude_did {
                            continue;
                        }
                    }
                    *blob_counts.entry(did.clone()).or_insert(0) += 1;
                }
            }
        }

        blob_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
    }

    /// Calculate data locality ratio for executor
    pub fn data_locality_ratio(&self, executor_did: &str, required_blobs: &[[u8; 32]]) -> f64 {
        if required_blobs.is_empty() {
            return 1.0; // No data requirements = perfect locality
        }

        let local_count = required_blobs
            .iter()
            .filter(|hash| {
                self.blob_locations
                    .get(*hash)
                    .map(|locs| locs.iter().any(|did| did == executor_did))
                    .unwrap_or(false)
            })
            .count();

        local_count as f64 / required_blobs.len() as f64
    }
}

/// Trait for implementing actor migration policies.
///
/// Different cooperatives can implement custom migration strategies.
pub trait MigrationPolicy: Send + Sync {
    /// Evaluate if actor should migrate.
    ///
    /// Returns `None` if no migration needed, or `Some(decision)` if migration beneficial.
    ///
    /// Called periodically by executor's migration evaluator (e.g., every 30 seconds).
    fn should_migrate(
        &self,
        actor_id: &ActorId,
        current_executor: &str,
        actor_state: &ActorRuntimeState,
        network_state: &NetworkState,
    ) -> Option<MigrationDecision>;

    /// Evaluate if this executor should accept an incoming migration request.
    ///
    /// Returns `true` if executor can accommodate actor, `false` to reject.
    fn should_accept_migration(
        &self,
        actor_id: &ActorId,
        from_executor: &str,
        estimated_memory_bytes: u64,
        current_capacity: &NodeCapacity,
        current_queue_depth: usize,
    ) -> bool;
}

/// Default migration policy implementation.
///
/// Balances load, locality, and trust.
#[derive(Debug, Clone)]
pub struct DefaultMigrationPolicy {
    /// Load threshold for triggering migration (0.0 - 1.0)
    pub load_threshold: f64,

    /// Minimum benefit score to justify migration (0.0 - 10.0)
    pub min_benefit: f64,

    /// Minimum trust score for migration target
    pub min_trust: f64,

    /// Data locality weight (0.0 - 1.0)
    pub locality_weight: f64,

    /// Load reduction weight (0.0 - 1.0)
    pub load_weight: f64,
}

impl Default for DefaultMigrationPolicy {
    fn default() -> Self {
        Self {
            load_threshold: 0.80, // Migrate when executor >80% loaded
            min_benefit: 3.0,     // Require benefit score >3.0 to migrate
            min_trust: 0.3,       // Require MIN_TRUST_EXECUTE
            locality_weight: 0.6, // 60% weight to data locality
            load_weight: 0.4,     // 40% weight to load reduction
        }
    }
}

impl MigrationPolicy for DefaultMigrationPolicy {
    fn should_migrate(
        &self,
        actor_id: &ActorId,
        current_executor: &str,
        actor_state: &ActorRuntimeState,
        network_state: &NetworkState,
    ) -> Option<MigrationDecision> {
        // 1. Check if current executor overloaded
        let should_load_balance = network_state.executor_load > self.load_threshold;

        // 2. Check data locality
        let current_locality =
            network_state.data_locality_ratio(current_executor, &actor_state.required_blobs);

        let should_optimize_locality =
            !actor_state.required_blobs.is_empty() && current_locality < 0.5;

        // If neither condition met, don't migrate
        if !should_load_balance && !should_optimize_locality {
            return None;
        }

        // 3. Find candidate executors
        let candidates: Vec<_> = network_state
            .available_executors
            .iter()
            .filter(|exec| exec.did != current_executor)
            .filter(|exec| exec.trust_score >= self.min_trust)
            .filter(|exec| exec.load < self.load_threshold * 0.8) // Target 80% of threshold
            .collect();

        if candidates.is_empty() {
            return None; // No suitable targets
        }

        // 4. Score candidates
        let mut best_executor: Option<(&ExecutorInfo, f64)> = None;

        for exec in candidates {
            let mut score = 0.0;

            // Load reduction benefit (0-10 scale)
            let load_delta = network_state.executor_load - exec.load;
            score += load_delta * 10.0 * self.load_weight;

            // Data locality benefit (0-10 scale)
            if !actor_state.required_blobs.is_empty() {
                let target_locality =
                    network_state.data_locality_ratio(&exec.did, &actor_state.required_blobs);
                let locality_improvement = target_locality - current_locality;
                score += locality_improvement * 10.0 * self.locality_weight;
            }

            // Trust bonus (small)
            score += exec.trust_score * 0.5;

            if let Some((_, best_score)) = best_executor {
                if score > best_score {
                    best_executor = Some((exec, score));
                }
            } else {
                best_executor = Some((exec, score));
            }
        }

        let (best, benefit_score) = best_executor?;

        // Check if migration is worth the cost
        if benefit_score < self.min_benefit {
            return None;
        }

        // Determine primary reason
        let reason = if should_load_balance {
            MigrationReason::LoadBalancing
        } else {
            MigrationReason::LocalityOptimization
        };

        // Estimate migration cost (checkpoint transfer)
        let state_size_mb = actor_state.memory_bytes / (1024 * 1024);
        let transfer_time_ms = state_size_mb * 10; // Heuristic: ~10ms per MB

        Some(MigrationDecision {
            actor_id: *actor_id,
            from_executor: current_executor.to_string(),
            target_executor: best.did.clone(),
            reason,
            cost_estimate_ms: transfer_time_ms,
            benefit_score,
        })
    }

    fn should_accept_migration(
        &self,
        _actor_id: &ActorId,
        _from_executor: &str,
        estimated_memory_bytes: u64,
        current_capacity: &NodeCapacity,
        current_queue_depth: usize,
    ) -> bool {
        // Check if we have enough memory
        let memory_mb = estimated_memory_bytes / (1024 * 1024);
        if current_capacity.memory_mb_available < memory_mb {
            return false;
        }

        // Check if queue not too deep
        if current_queue_depth > 20 {
            return false;
        }

        true // Accept migration
    }
}

/// Locality-first migration policy (for data-intensive workloads).
///
/// Prioritizes data locality over load balancing.
#[derive(Debug, Clone)]
pub struct LocalityFirstPolicy {
    pub min_trust: f64,
    pub min_locality_improvement: f64, // Minimum improvement to justify migration
}

impl Default for LocalityFirstPolicy {
    fn default() -> Self {
        Self {
            min_trust: 0.3,
            min_locality_improvement: 0.3, // Require 30% improvement
        }
    }
}

impl MigrationPolicy for LocalityFirstPolicy {
    fn should_migrate(
        &self,
        actor_id: &ActorId,
        current_executor: &str,
        actor_state: &ActorRuntimeState,
        network_state: &NetworkState,
    ) -> Option<MigrationDecision> {
        // Only consider migration if actor has data requirements
        if actor_state.required_blobs.is_empty() {
            return None;
        }

        let current_locality =
            network_state.data_locality_ratio(current_executor, &actor_state.required_blobs);

        // Find executor with best data locality
        let (best_executor, local_count) =
            network_state.executor_with_most_blobs(&actor_state.required_blobs, Some(current_executor))?;

        let best_locality = local_count as f64 / actor_state.required_blobs.len() as f64;
        let improvement = best_locality - current_locality;

        // Check if improvement significant enough
        if improvement < self.min_locality_improvement {
            return None;
        }

        // Check target executor trust
        let target_info = network_state
            .available_executors
            .iter()
            .find(|exec| exec.did == best_executor)?;

        if target_info.trust_score < self.min_trust {
            return None;
        }

        // Estimate cost
        let state_size_mb = actor_state.memory_bytes / (1024 * 1024);
        let transfer_time_ms = state_size_mb * 10;

        Some(MigrationDecision {
            actor_id: *actor_id,
            from_executor: current_executor.to_string(),
            target_executor: best_executor,
            reason: MigrationReason::LocalityOptimization,
            cost_estimate_ms: transfer_time_ms,
            benefit_score: improvement * 10.0, // Scale to 0-10
        })
    }

    fn should_accept_migration(
        &self,
        _actor_id: &ActorId,
        _from_executor: &str,
        estimated_memory_bytes: u64,
        current_capacity: &NodeCapacity,
        _current_queue_depth: usize,
    ) -> bool {
        // Only check memory capacity
        let memory_mb = estimated_memory_bytes / (1024 * 1024);
        current_capacity.memory_mb_available >= memory_mb
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::GpuDevice;

    fn make_executor(did: &str, trust: f64, load: f64) -> ExecutorInfo {
        ExecutorInfo {
            did: did.to_string(),
            trust_score: trust,
            load,
            capacity: NodeCapacity {
                cpu_cores_total: 8.0,
                cpu_cores_available: 4.0,
                memory_mb_total: 16384,
                memory_mb_available: 8192,
                storage_mb_available: 100_000,
                network_mbps: 1000.0,
                gpu_devices: vec![],
                updated_at: 1000,
            },
            queue_depth: 0,
            last_seen: 1000,
        }
    }

    fn make_network_state(executor_load: f64, executors: Vec<ExecutorInfo>) -> NetworkState {
        NetworkState {
            executor_load,
            available_executors: executors,
            executor_rtt: HashMap::new(),
            blob_locations: HashMap::new(),
            snapshot_at: 1000,
        }
    }

    #[test]
    fn test_default_policy_no_migration_under_threshold() {
        let policy = DefaultMigrationPolicy::default();
        let actor_id = [1u8; 32];
        let current_executor = "did:icn:executor-a";

        let actor_state = ActorRuntimeState {
            actor_id,
            uptime_secs: 60,
            memory_bytes: 1024 * 1024,
            cpu_utilization: 0.5,
            msg_throughput: 10.0,
            last_checkpoint_at: 1000,
            checkpoint_sequence: 5,
            required_blobs: vec![],
        };

        // Load below threshold (70% < 80%)
        let network_state = make_network_state(
            0.70,
            vec![
                make_executor("did:icn:executor-a", 0.8, 0.70),
                make_executor("did:icn:executor-b", 0.9, 0.30),
            ],
        );

        let decision = policy.should_migrate(&actor_id, current_executor, &actor_state, &network_state);

        assert!(decision.is_none(), "Should not migrate when load below threshold");
    }

    #[test]
    fn test_default_policy_migrate_on_overload() {
        let policy = DefaultMigrationPolicy::default();
        let actor_id = [2u8; 32];
        let current_executor = "did:icn:executor-a";

        let actor_state = ActorRuntimeState {
            actor_id,
            uptime_secs: 120,
            memory_bytes: 2 * 1024 * 1024,
            cpu_utilization: 0.9,
            msg_throughput: 50.0,
            last_checkpoint_at: 2000,
            checkpoint_sequence: 10,
            required_blobs: vec![],
        };

        // Load above threshold (90% > 80%), much larger delta to meet benefit threshold
        let network_state = make_network_state(
            0.90,
            vec![
                make_executor("did:icn:executor-a", 0.8, 0.90),
                make_executor("did:icn:executor-b", 0.9, 0.10), // Very low load
            ],
        );

        let decision = policy.should_migrate(&actor_id, current_executor, &actor_state, &network_state);

        assert!(decision.is_some(), "Should migrate when overloaded");
        let dec = decision.unwrap();
        assert_eq!(dec.target_executor, "did:icn:executor-b");
        assert_eq!(dec.reason, MigrationReason::LoadBalancing);
        // Benefit calculation: (0.90 - 0.10) * 10.0 * 0.4 (load_weight) + 0.9 * 0.5 (trust) = 3.2 + 0.45 = 3.65
        assert!(dec.benefit_score >= policy.min_benefit);
    }

    #[test]
    fn test_default_policy_respects_min_trust() {
        let policy = DefaultMigrationPolicy::default();
        let actor_id = [3u8; 32];
        let current_executor = "did:icn:executor-a";

        let actor_state = ActorRuntimeState {
            actor_id,
            uptime_secs: 60,
            memory_bytes: 1024 * 1024,
            cpu_utilization: 0.5,
            msg_throughput: 10.0,
            last_checkpoint_at: 1000,
            checkpoint_sequence: 5,
            required_blobs: vec![],
        };

        // Only available target has low trust (0.2 < 0.3)
        let network_state = make_network_state(
            0.85,
            vec![
                make_executor("did:icn:executor-a", 0.8, 0.85),
                make_executor("did:icn:executor-b", 0.2, 0.30), // Low trust
            ],
        );

        let decision = policy.should_migrate(&actor_id, current_executor, &actor_state, &network_state);

        assert!(decision.is_none(), "Should not migrate to low-trust executor");
    }

    #[test]
    fn test_locality_first_policy() {
        let policy = LocalityFirstPolicy::default();
        let actor_id = [4u8; 32];
        let current_executor = "did:icn:executor-a";

        let blob1 = [10u8; 32];
        let blob2 = [20u8; 32];
        let blob3 = [30u8; 32];

        let actor_state = ActorRuntimeState {
            actor_id,
            uptime_secs: 60,
            memory_bytes: 1024 * 1024,
            cpu_utilization: 0.5,
            msg_throughput: 10.0,
            last_checkpoint_at: 1000,
            checkpoint_sequence: 5,
            required_blobs: vec![blob1, blob2, blob3],
        };

        // Current executor has 1/3 blobs, target has 3/3 blobs
        let mut blob_locations = HashMap::new();
        blob_locations.insert(blob1, vec!["did:icn:executor-a".to_string(), "did:icn:executor-b".to_string()]);
        blob_locations.insert(blob2, vec!["did:icn:executor-b".to_string()]);
        blob_locations.insert(blob3, vec!["did:icn:executor-b".to_string()]);

        let network_state = NetworkState {
            executor_load: 0.50,
            available_executors: vec![
                make_executor("did:icn:executor-a", 0.8, 0.50),
                make_executor("did:icn:executor-b", 0.9, 0.50),
            ],
            executor_rtt: HashMap::new(),
            blob_locations,
            snapshot_at: 1000,
        };

        let decision = policy.should_migrate(&actor_id, current_executor, &actor_state, &network_state);

        assert!(decision.is_some(), "Should migrate for better locality");
        let dec = decision.unwrap();
        assert_eq!(dec.target_executor, "did:icn:executor-b");
        assert_eq!(dec.reason, MigrationReason::LocalityOptimization);

        // Improvement: from 1/3 (0.33) to 3/3 (1.0) = 0.67 improvement
        // Benefit score: 0.67 * 10 = 6.7
        assert!(dec.benefit_score > 6.0);
    }

    #[test]
    fn test_network_state_data_locality_ratio() {
        let blob1 = [1u8; 32];
        let blob2 = [2u8; 32];
        let blob3 = [3u8; 32];

        let mut blob_locations = HashMap::new();
        blob_locations.insert(blob1, vec!["did:icn:executor-a".to_string()]);
        blob_locations.insert(blob2, vec!["did:icn:executor-a".to_string()]);
        blob_locations.insert(blob3, vec!["did:icn:executor-b".to_string()]);

        let network_state = NetworkState {
            executor_load: 0.5,
            available_executors: vec![],
            executor_rtt: HashMap::new(),
            blob_locations,
            snapshot_at: 1000,
        };

        // Executor A has 2/3 blobs
        let ratio_a = network_state.data_locality_ratio("did:icn:executor-a", &[blob1, blob2, blob3]);
        assert!((ratio_a - 0.666).abs() < 0.01);

        // Executor B has 1/3 blobs
        let ratio_b = network_state.data_locality_ratio("did:icn:executor-b", &[blob1, blob2, blob3]);
        assert!((ratio_b - 0.333).abs() < 0.01);

        // No blobs required = perfect locality
        let ratio_none = network_state.data_locality_ratio("did:icn:executor-c", &[]);
        assert_eq!(ratio_none, 1.0);
    }

    #[test]
    fn test_should_accept_migration() {
        let policy = DefaultMigrationPolicy::default();
        let actor_id = [5u8; 32];

        let capacity_ok = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192, // 8GB available
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        // Accept: enough memory (2GB < 8GB) and low queue
        assert!(policy.should_accept_migration(
            &actor_id,
            "did:icn:source",
            2 * 1024 * 1024 * 1024, // 2GB
            &capacity_ok,
            5
        ));

        // Reject: not enough memory (10GB > 8GB)
        assert!(!policy.should_accept_migration(
            &actor_id,
            "did:icn:source",
            10 * 1024 * 1024 * 1024, // 10GB
            &capacity_ok,
            5
        ));

        // Reject: queue too deep (25 > 20)
        assert!(!policy.should_accept_migration(
            &actor_id,
            "did:icn:source",
            2 * 1024 * 1024 * 1024, // 2GB
            &capacity_ok,
            25
        ));
    }
}
