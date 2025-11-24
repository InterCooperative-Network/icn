//! Scheduler primitives for intelligent task placement.
//!
//! This module extends the Phase 15 compute layer with resource-aware scheduling.
//! It introduces resource profiles, capacity tracking, and placement scoring.
//!
//! # Design Philosophy
//!
//! - **Incremental**: Builds on existing task submission/claiming protocol
//! - **Backward Compatible**: Works alongside legacy claiming logic
//! - **Trust-Governed**: Trust scores remain primary scheduling input
//! - **Gossip-Based**: No centralized scheduler, distributed negotiation
//!
//! # Evolution Stages
//!
//! - **Phase 16A** (this module): Resource profiles and capacity matching
//! - **Phase 16B**: Placement scoring and deliberation windows
//! - **Phase 16C**: Locality awareness and topology integration
//! - **Phase 16D**: Actor state and migration
//! - **Phase 16E**: Cooperative scheduling policies

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Resource requirements for a compute task or actor.
///
/// This replaces vague "capabilities" with concrete resource needs.
/// All fields are optional to support incremental adoption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceProfile {
    /// CPU cores required (e.g., 0.5 = half a core, 2.0 = two cores)
    pub cpu_cores: Option<f64>,

    /// Memory required in megabytes
    pub memory_mb: Option<u64>,

    /// Temporary storage required in megabytes
    pub storage_mb: Option<u64>,

    /// Network bandwidth required in Mbps
    pub network_mbps: Option<f64>,

    /// GPU requirements (if any)
    pub gpu_spec: Option<GpuSpec>,

    /// Expected runtime duration (helps with scheduling)
    pub duration_estimate: Option<Duration>,
}

impl ResourceProfile {
    /// Create a minimal profile (backward compatible with existing tasks)
    pub fn minimal() -> Self {
        Self {
            cpu_cores: Some(0.1), // 10% of one core
            memory_mb: Some(128), // 128 MB
            storage_mb: Some(10), // 10 MB temp
            network_mbps: None,
            gpu_spec: None,
            duration_estimate: None,
        }
    }

    /// Create a profile for compute-intensive tasks
    pub fn compute_heavy(cores: f64, memory_mb: u64) -> Self {
        Self {
            cpu_cores: Some(cores),
            memory_mb: Some(memory_mb),
            storage_mb: Some(100),
            network_mbps: None,
            gpu_spec: None,
            duration_estimate: None,
        }
    }

    /// Create a profile for GPU tasks
    pub fn gpu(memory_gb: u64, compute_capability: String) -> Self {
        Self {
            cpu_cores: Some(1.0),
            memory_mb: Some(2048),
            storage_mb: Some(100),
            network_mbps: None,
            gpu_spec: Some(GpuSpec {
                memory_gb,
                compute_capability,
                device_count: 1,
            }),
            duration_estimate: None,
        }
    }

    /// Validate that this profile is reasonable
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(cores) = self.cpu_cores {
            if cores <= 0.0 || cores > 128.0 {
                return Err("CPU cores must be between 0 and 128");
            }
        }

        if let Some(mem) = self.memory_mb {
            if mem == 0 || mem > 1_000_000 {
                // 1TB max
                return Err("Memory must be between 1 MB and 1 TB");
            }
        }

        if let Some(storage) = self.storage_mb {
            if storage > 10_000_000 {
                // 10TB max
                return Err("Storage must be less than 10 TB");
            }
        }

        Ok(())
    }
}

impl Default for ResourceProfile {
    fn default() -> Self {
        Self::minimal()
    }
}

/// GPU requirements specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuSpec {
    /// GPU memory required in GB
    pub memory_gb: u64,

    /// Minimum compute capability (e.g., "sm_80" for A100)
    pub compute_capability: String,

    /// Number of GPU devices required
    pub device_count: usize,
}

/// Node's available compute resources.
///
/// Periodically announced via gossip to inform placement decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeCapacity {
    /// Total CPU cores on this node
    pub cpu_cores_total: f64,

    /// Currently available CPU cores
    pub cpu_cores_available: f64,

    /// Total memory in MB
    pub memory_mb_total: u64,

    /// Available memory in MB
    pub memory_mb_available: u64,

    /// Available storage in MB
    pub storage_mb_available: u64,

    /// Network bandwidth in Mbps
    pub network_mbps: f64,

    /// GPU devices available
    pub gpu_devices: Vec<GpuDevice>,

    /// When this capacity snapshot was taken
    pub updated_at: u64,
}

impl NodeCapacity {
    /// Check if this node can fit a task with given profile
    pub fn can_fit(&self, profile: &ResourceProfile) -> bool {
        // Check CPU
        if let Some(required_cpu) = profile.cpu_cores {
            if self.cpu_cores_available < required_cpu {
                return false;
            }
        }

        // Check memory
        if let Some(required_mem) = profile.memory_mb {
            if self.memory_mb_available < required_mem {
                return false;
            }
        }

        // Check storage
        if let Some(required_storage) = profile.storage_mb {
            if self.storage_mb_available < required_storage {
                return false;
            }
        }

        // Check network
        if let Some(required_bw) = profile.network_mbps {
            if self.network_mbps < required_bw {
                return false;
            }
        }

        // Check GPU
        if let Some(ref required_gpu) = profile.gpu_spec {
            let available_gpus = self
                .gpu_devices
                .iter()
                .filter(|gpu| {
                    gpu.available
                        && gpu.memory_gb >= required_gpu.memory_gb
                        && gpu.compute_capability >= required_gpu.compute_capability
                })
                .count();

            if available_gpus < required_gpu.device_count {
                return false;
            }
        }

        true
    }

    /// Calculate available capacity as ratio (0.0 = full, 1.0 = empty)
    pub fn available_ratio(&self) -> f64 {
        let cpu_ratio = self.cpu_cores_available / self.cpu_cores_total;
        let mem_ratio = self.memory_mb_available as f64 / self.memory_mb_total as f64;

        // Return minimum (most constrained resource)
        cpu_ratio.min(mem_ratio)
    }

    /// Reserve resources for a task
    pub fn reserve(&mut self, profile: &ResourceProfile) -> Result<(), &'static str> {
        if !self.can_fit(profile) {
            return Err("Insufficient capacity");
        }

        if let Some(cpu) = profile.cpu_cores {
            self.cpu_cores_available -= cpu;
        }

        if let Some(mem) = profile.memory_mb {
            self.memory_mb_available -= mem;
        }

        if let Some(storage) = profile.storage_mb {
            self.storage_mb_available -= storage;
        }

        // Mark GPU as unavailable
        if let Some(ref gpu_spec) = profile.gpu_spec {
            let mut reserved = 0;
            for gpu in &mut self.gpu_devices {
                if gpu.available
                    && gpu.memory_gb >= gpu_spec.memory_gb
                    && gpu.compute_capability >= gpu_spec.compute_capability
                {
                    gpu.available = false;
                    reserved += 1;
                    if reserved >= gpu_spec.device_count {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Release resources after task completion
    pub fn release(&mut self, profile: &ResourceProfile) {
        if let Some(cpu) = profile.cpu_cores {
            self.cpu_cores_available += cpu;
            self.cpu_cores_available = self.cpu_cores_available.min(self.cpu_cores_total);
        }

        if let Some(mem) = profile.memory_mb {
            self.memory_mb_available += mem;
            self.memory_mb_available = self.memory_mb_available.min(self.memory_mb_total);
        }

        if let Some(storage) = profile.storage_mb {
            self.storage_mb_available += storage;
        }

        // Release GPU
        if let Some(ref gpu_spec) = profile.gpu_spec {
            let mut released = 0;
            for gpu in &mut self.gpu_devices {
                if !gpu.available
                    && gpu.memory_gb >= gpu_spec.memory_gb
                    && gpu.compute_capability >= gpu_spec.compute_capability
                {
                    gpu.available = true;
                    released += 1;
                    if released >= gpu_spec.device_count {
                        break;
                    }
                }
            }
        }
    }
}

/// GPU device information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuDevice {
    /// Device identifier
    pub device_id: String,

    /// GPU memory in GB
    pub memory_gb: u64,

    /// Compute capability (e.g., "sm_80")
    pub compute_capability: String,

    /// Device name (e.g., "NVIDIA A100")
    pub device_name: String,

    /// Currently available for allocation
    pub available: bool,
}

/// Locality hints for task placement.
///
/// These guide the scheduler to prefer certain execution locations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LocalityHint {
    /// Prefer execution on specific DID
    PreferDid(String),

    /// Prefer execution in geographic region
    PreferRegion(String),

    /// Place near data blobs (minimize transfer)
    DataLocality(Vec<[u8; 32]>),

    /// Avoid specific DID (blacklist)
    AvoidDid(String),

    /// Colocate with another task
    ColocateWith([u8; 32]),
}

/// Placement request for a task.
///
/// Sent via gossip to solicit bids from executors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRequest {
    /// The task to place
    pub task_hash: [u8; 32],

    /// Resource requirements
    pub resource_profile: ResourceProfile,

    /// Locality preferences
    pub locality_hints: Vec<LocalityHint>,

    /// Maximum cost willing to pay (credits per 1000 fuel)
    pub max_cost: Option<u64>,

    /// When this request was sent
    pub requested_at: u64,
}

/// Executor's offer to run a task.
///
/// Higher score = better fit. Scheduler picks highest score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementOffer {
    /// Executor's DID
    pub executor: String,

    /// Placement score (0.0 - 1.0, higher = better)
    pub score: f64,

    /// Cost in credits per 1000 fuel
    pub cost: u64,

    /// Estimated start time (Unix millis)
    pub estimated_start: u64,

    /// When this offer was made
    pub offered_at: u64,
}

impl PartialOrd for PlacementOffer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.score.partial_cmp(&other.score)
    }
}

/// State needed for placement decisions.
///
/// Maintained by each executor to score incoming tasks.
#[derive(Debug, Clone)]
pub struct NodeState {
    /// This node's DID
    pub did: String,

    /// Current resource capacity
    pub capacity: NodeCapacity,

    /// Tasks currently executing
    pub executing_tasks: HashMap<[u8; 32], ResourceProfile>,

    /// Queue depth (number of pending tasks)
    pub queue_depth: usize,
}

impl NodeState {
    /// Estimate queue wait time in milliseconds
    pub fn queue_depth_ms(&self) -> u64 {
        // Rough estimate: 10 seconds per queued task
        (self.queue_depth as u64) * 10_000
    }

    /// Update capacity based on executing tasks
    pub fn refresh_capacity(&mut self) {
        // This would be called periodically to update capacity
        // based on actual resource usage
        //
        // In real implementation, integrate with system metrics
        // (e.g., /proc/stat, sysinfo crate)
    }
}

/// Network locality context for placement scoring (Phase 16C).
///
/// Provides network measurements and data location information
/// to enable locality-aware task placement.
#[derive(Debug, Clone)]
pub struct LocalityContext {
    /// RTT to submitter in milliseconds (if known)
    pub submitter_rtt_ms: Option<u64>,

    /// Number of input blobs that are locally available
    pub local_blob_count: usize,

    /// Total number of input blobs requested
    pub total_blob_count: usize,

    /// Node's geographic region (if topology enabled)
    pub own_region: Option<String>,

    /// Submitter's geographic region (if known)
    pub submitter_region: Option<String>,
}

impl LocalityContext {
    /// Create empty context (no locality information)
    pub fn empty() -> Self {
        Self {
            submitter_rtt_ms: None,
            local_blob_count: 0,
            total_blob_count: 0,
            own_region: None,
            submitter_region: None,
        }
    }

    /// Calculate data locality ratio (0.0 - 1.0)
    ///
    /// Returns the fraction of input blobs available locally.
    /// Returns 0.0 if blob information is unknown (total_blob_count = 0).
    /// This prevents rewarding executors when blob locality is unverified.
    pub fn data_locality_ratio(&self) -> f64 {
        if self.total_blob_count == 0 {
            return 0.0; // Unknown blobs = no locality bonus
        }
        self.local_blob_count as f64 / self.total_blob_count as f64
    }

    /// Check if submitter is in same region
    pub fn same_region(&self) -> bool {
        match (&self.own_region, &self.submitter_region) {
            (Some(own), Some(sub)) => own == sub,
            _ => false,
        }
    }
}

/// Trait for implementing placement policies.
///
/// Different cooperatives can implement custom scoring logic.
pub trait PlacementPolicy: Send + Sync {
    /// Score a task for execution on this node.
    ///
    /// Returns None if this node cannot or should not execute the task.
    /// Returns Some(offer) with a score between 0.0 and 1.0.
    ///
    /// # Phase 16C Enhancement
    ///
    /// Added `locality_hints` and `locality_ctx` parameters for network-aware scoring:
    /// - `locality_hints`: Task placement preferences (region, data locality, etc.)
    /// - `locality_ctx`: Network context for computing locality scores (RTT, blob locations)
    fn score_task(
        &self,
        task_hash: &[u8; 32],
        profile: &ResourceProfile,
        submitter: &str,
        node_state: &NodeState,
        trust_score: f64,
        locality_hints: &[LocalityHint],
        locality_ctx: &LocalityContext,
    ) -> Option<PlacementOffer>;
}

/// Default placement policy implementation.
///
/// Balances trust, capacity, and economics.
pub struct DefaultPlacementPolicy {
    /// Minimum trust score to consider execution
    pub min_trust: f64,

    /// Base cost per 1000 fuel units
    pub base_cost: u64,

    /// Load factor multiplier when node is busy
    pub load_multiplier: f64,
}

impl Default for DefaultPlacementPolicy {
    fn default() -> Self {
        Self {
            min_trust: 0.3, // MIN_TRUST_EXECUTE from Phase 15
            base_cost: 10,  // 10 credits per 1000 fuel
            load_multiplier: 1.5,
        }
    }
}

impl PlacementPolicy for DefaultPlacementPolicy {
    fn score_task(
        &self,
        _task_hash: &[u8; 32],
        profile: &ResourceProfile,
        _submitter: &str,
        node_state: &NodeState,
        trust_score: f64,
        locality_hints: &[LocalityHint],
        locality_ctx: &LocalityContext,
    ) -> Option<PlacementOffer> {
        // Trust gate
        if trust_score < self.min_trust {
            return None;
        }

        // Capacity check
        if !node_state.capacity.can_fit(profile) {
            return None;
        }

        // Compute score (0.0 - 1.0)
        // Phase 16C: Rebalanced weights to include locality factors
        let mut score = 0.0;

        // Factor 1: Trust (weight 0.25, reduced from 0.4)
        // Scale trust score to 0-0.25 range
        score += (trust_score * 0.25).min(0.25);

        // Factor 2: Available capacity (weight 0.20, reduced from 0.3)
        // Prefer nodes with more available resources
        let capacity_ratio = node_state.capacity.available_ratio();
        score += capacity_ratio * 0.20;

        // Factor 3: Queue depth (weight 0.15, reduced from 0.2)
        // Prefer nodes with shorter queues
        let queue_penalty = (node_state.queue_depth as f64 / 10.0).min(1.0);
        score += (1.0 - queue_penalty) * 0.15;

        // Factor 4: Network latency (weight 0.15, NEW in Phase 16C)
        // Prefer nodes with lower RTT to submitter
        if let Some(rtt_ms) = locality_ctx.submitter_rtt_ms {
            // Convert RTT to score: lower RTT = higher score
            // 0ms = 1.0, 500ms = 0.0 (linear interpolation)
            let rtt_score = (1.0 - (rtt_ms as f64 / 500.0).min(1.0)).max(0.0);
            score += rtt_score * 0.15;
        }
        // If no RTT data, no bonus (neutral)

        // Factor 5: Data locality (weight 0.15, NEW in Phase 16C)
        // Prefer nodes that already have input blobs
        let data_locality_ratio = locality_ctx.data_locality_ratio();
        score += data_locality_ratio * 0.15;

        // Factor 6: Locality hints bonus (weight 0.10, NEW in Phase 16C)
        // Apply bonus if we match submitter preferences
        let mut hint_bonus: f64 = 0.0;
        for hint in locality_hints {
            match hint {
                LocalityHint::PreferRegion(region) => {
                    if let Some(ref own_region) = locality_ctx.own_region {
                        if own_region == region {
                            hint_bonus += 0.05; // 5% bonus for region match
                        }
                    }
                }
                LocalityHint::PreferDid(did) => {
                    if &node_state.did == did {
                        hint_bonus += 0.10; // 10% bonus for direct DID preference
                    }
                }
                LocalityHint::AvoidDid(did) => {
                    if &node_state.did == did {
                        return None; // Hard reject if blacklisted
                    }
                }
                _ => {
                    // DataLocality and ColocateWith handled via locality_ctx
                }
            }
        }
        score += hint_bonus.min(0.10); // Cap hint bonus at 10%

        // Factor 7: Random jitter (weight 0.10, same as before, but rebalanced)
        // Break ties and prevent thundering herd
        use rand::Rng;
        let jitter = rand::thread_rng().gen::<f64>() * 0.10;
        score += jitter;

        // Total weights: 0.25 + 0.20 + 0.15 + 0.15 + 0.15 + 0.10 + 0.10 = 1.10 max (with bonuses)
        // Actual max without bonuses: 1.00
        // Cap final score at 1.0
        score = score.min(1.0);

        // Calculate cost (base cost + load multiplier)
        let load_factor = if node_state.queue_depth > 5 {
            self.load_multiplier
        } else {
            1.0
        };
        let cost = (self.base_cost as f64 * load_factor) as u64;

        // Estimated start time
        let estimated_start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + node_state.queue_depth_ms();

        Some(PlacementOffer {
            executor: node_state.did.clone(),
            score,
            cost,
            estimated_start,
            offered_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_profile_minimal() {
        let profile = ResourceProfile::minimal();
        assert_eq!(profile.cpu_cores, Some(0.1));
        assert_eq!(profile.memory_mb, Some(128));
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn test_resource_profile_validation() {
        let mut profile = ResourceProfile::minimal();

        // Valid
        assert!(profile.validate().is_ok());

        // Invalid CPU
        profile.cpu_cores = Some(0.0);
        assert!(profile.validate().is_err());

        profile.cpu_cores = Some(200.0);
        assert!(profile.validate().is_err());

        profile.cpu_cores = Some(4.0);

        // Invalid memory
        profile.memory_mb = Some(0);
        assert!(profile.validate().is_err());

        profile.memory_mb = Some(2_000_000); // 2TB
        assert!(profile.validate().is_err());
    }

    #[test]
    fn test_node_capacity_can_fit() {
        let capacity = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        // Fits easily
        let small_profile = ResourceProfile {
            cpu_cores: Some(1.0),
            memory_mb: Some(1024),
            storage_mb: Some(100),
            network_mbps: None,
            gpu_spec: None,
            duration_estimate: None,
        };
        assert!(capacity.can_fit(&small_profile));

        // Too much CPU
        let large_cpu = ResourceProfile {
            cpu_cores: Some(8.0),
            ..small_profile.clone()
        };
        assert!(!capacity.can_fit(&large_cpu));

        // Too much memory
        let large_mem = ResourceProfile {
            memory_mb: Some(10_000),
            ..small_profile.clone()
        };
        assert!(!capacity.can_fit(&large_mem));
    }

    #[test]
    fn test_node_capacity_reserve_release() {
        let mut capacity = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 8.0,
            memory_mb_total: 16384,
            memory_mb_available: 16384,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let profile = ResourceProfile {
            cpu_cores: Some(2.0),
            memory_mb: Some(4096),
            storage_mb: Some(1000),
            network_mbps: None,
            gpu_spec: None,
            duration_estimate: None,
        };

        // Reserve
        assert!(capacity.reserve(&profile).is_ok());
        assert_eq!(capacity.cpu_cores_available, 6.0);
        assert_eq!(capacity.memory_mb_available, 12288);
        assert_eq!(capacity.storage_mb_available, 99_000);

        // Release
        capacity.release(&profile);
        assert_eq!(capacity.cpu_cores_available, 8.0);
        assert_eq!(capacity.memory_mb_available, 16384);
        assert_eq!(capacity.storage_mb_available, 100_000);
    }

    #[test]
    fn test_default_placement_policy() {
        let policy = DefaultPlacementPolicy::default();

        let profile = ResourceProfile::minimal();

        let node_state = NodeState {
            did: "did:icn:executor".into(),
            capacity: NodeCapacity {
                cpu_cores_total: 8.0,
                cpu_cores_available: 6.0,
                memory_mb_total: 16384,
                memory_mb_available: 12288,
                storage_mb_available: 100_000,
                network_mbps: 1000.0,
                gpu_devices: vec![],
                updated_at: 1000,
            },
            executing_tasks: HashMap::new(),
            queue_depth: 2,
        };

        let task_hash = [0u8; 32];
        let empty_locality = LocalityContext::empty();
        let no_hints: Vec<LocalityHint> = vec![];

        // High trust, should get good score
        let offer = policy
            .score_task(&task_hash, &profile, "did:icn:alice", &node_state, 0.8, &no_hints, &empty_locality)
            .unwrap();

        assert!(offer.score > 0.4); // Trust (0.20) + capacity (0.15) + queue (0.12) + jitter
        assert_eq!(offer.cost, 10); // Base cost

        // Low trust, rejected
        let offer = policy.score_task(&task_hash, &profile, "did:icn:untrusted", &node_state, 0.1, &no_hints, &empty_locality);
        assert!(offer.is_none());

        // High queue, cost multiplier kicks in
        let busy_node = NodeState {
            queue_depth: 10,
            ..node_state.clone()
        };

        let offer = policy
            .score_task(&task_hash, &profile, "did:icn:alice", &busy_node, 0.8, &no_hints, &empty_locality)
            .unwrap();

        assert_eq!(offer.cost, 15); // base_cost * load_multiplier

        // Test with locality context (Phase 16C)
        let locality_with_rtt = LocalityContext {
            submitter_rtt_ms: Some(50), // 50ms RTT
            local_blob_count: 0,
            total_blob_count: 0,
            own_region: None,
            submitter_region: None,
        };

        let offer_with_rtt = policy
            .score_task(&task_hash, &profile, "did:icn:alice", &node_state, 0.8, &no_hints, &locality_with_rtt)
            .unwrap();

        // Should score higher than without RTT info (RTT bonus: 0.9 * 0.15 = 0.135)
        // Use 0.05 threshold to account for random jitter variance (0-0.10 per invocation)
        assert!(offer_with_rtt.score > offer.score + 0.05);

        // Test with data locality
        let locality_with_data = LocalityContext {
            submitter_rtt_ms: None,
            local_blob_count: 3,
            total_blob_count: 3, // All blobs available locally
            own_region: None,
            submitter_region: None,
        };

        let offer_with_data = policy
            .score_task(&task_hash, &profile, "did:icn:alice", &node_state, 0.8, &no_hints, &locality_with_data)
            .unwrap();

        // Should get full data locality bonus (1.0 * 0.15 = 0.15)
        // Use 0.05 threshold to account for random jitter variance (0-0.10 per invocation)
        assert!(offer_with_data.score > offer.score + 0.05);
    }

    #[test]
    fn test_locality_context_data_locality_ratio() {
        // Test: Unknown blobs (total_blob_count = 0) should return 0.0, not 1.0
        let unknown_blobs = LocalityContext {
            submitter_rtt_ms: None,
            local_blob_count: 0,
            total_blob_count: 0,
            own_region: None,
            submitter_region: None,
        };
        assert_eq!(
            unknown_blobs.data_locality_ratio(),
            0.0,
            "Unknown blob information should not grant locality bonus"
        );

        // Test: All blobs available locally = 1.0
        let all_local = LocalityContext {
            submitter_rtt_ms: None,
            local_blob_count: 5,
            total_blob_count: 5,
            own_region: None,
            submitter_region: None,
        };
        assert_eq!(all_local.data_locality_ratio(), 1.0);

        // Test: Half blobs available locally = 0.5
        let half_local = LocalityContext {
            submitter_rtt_ms: None,
            local_blob_count: 3,
            total_blob_count: 6,
            own_region: None,
            submitter_region: None,
        };
        assert_eq!(half_local.data_locality_ratio(), 0.5);

        // Test: No blobs available locally = 0.0
        let none_local = LocalityContext {
            submitter_rtt_ms: None,
            local_blob_count: 0,
            total_blob_count: 10,
            own_region: None,
            submitter_region: None,
        };
        assert_eq!(none_local.data_locality_ratio(), 0.0);
    }

    #[test]
    fn test_gpu_capacity_matching() {
        let gpu_device = GpuDevice {
            device_id: "GPU-0".into(),
            memory_gb: 40,
            compute_capability: "sm_80".into(),
            device_name: "NVIDIA A100".into(),
            available: true,
        };

        let mut capacity = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 8.0,
            memory_mb_total: 16384,
            memory_mb_available: 16384,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![gpu_device.clone()],
            updated_at: 1000,
        };

        let gpu_profile = ResourceProfile::gpu(24, "sm_70".into());

        // Should fit (A100 has 40GB, sm_80 >= sm_70)
        assert!(capacity.can_fit(&gpu_profile));

        // Reserve GPU
        capacity.reserve(&gpu_profile).unwrap();
        assert!(!capacity.gpu_devices[0].available);

        // Can't fit another GPU task
        assert!(!capacity.can_fit(&gpu_profile));

        // Release GPU
        capacity.release(&gpu_profile);
        assert!(capacity.gpu_devices[0].available);
    }
}
