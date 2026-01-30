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

use icn_kernel_api::{CellId, ScopeLevel};
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

/// Configuration for periodic resource profile refresh
#[derive(Debug, Clone)]
pub struct ResourceRefreshConfig {
    /// Interval between resource sensing in seconds
    /// Default: 300 (5 minutes)
    pub refresh_interval_secs: u64,

    /// Threshold for detecting significant changes (0.0-1.0)
    /// Changes below this threshold are ignored
    /// Default: 0.1 (10% change)
    pub change_threshold: f64,
}

impl Default for ResourceRefreshConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 300, // 5 minutes
            change_threshold: 0.1,      // 10% change
        }
    }
}

impl ResourceRefreshConfig {
    /// Create a validated configuration with custom values.
    ///
    /// # Arguments
    /// * `refresh_interval_secs` - Interval between refreshes (must be > 0)
    /// * `change_threshold` - Threshold for detecting changes (must be in 0.0..=1.0)
    ///
    /// # Returns
    /// `Ok(config)` if values are valid, `Err` with explanation otherwise.
    pub fn new(refresh_interval_secs: u64, change_threshold: f64) -> Result<Self, &'static str> {
        if refresh_interval_secs == 0 {
            return Err("refresh_interval_secs must be > 0");
        }
        if !(0.0..=1.0).contains(&change_threshold) {
            return Err("change_threshold must be in range [0.0, 1.0]");
        }
        Ok(Self {
            refresh_interval_secs,
            change_threshold,
        })
    }

    /// Create config with custom refresh interval
    ///
    /// # Panics
    /// Panics if interval_secs is 0 (use `new()` for fallible construction)
    pub fn with_interval(interval_secs: u64) -> Self {
        assert!(interval_secs > 0, "refresh_interval_secs must be > 0");
        Self {
            refresh_interval_secs: interval_secs,
            ..Default::default()
        }
    }

    /// Create config with custom change threshold
    ///
    /// # Panics
    /// Panics if threshold is outside [0.0, 1.0] (use `new()` for fallible construction)
    pub fn with_threshold(threshold: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&threshold),
            "change_threshold must be in range [0.0, 1.0]"
        );
        Self {
            change_threshold: threshold,
            ..Default::default()
        }
    }
}

/// Type of resource change detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceChangeType {
    /// CPU availability changed significantly
    Cpu,
    /// Memory availability changed significantly
    Memory,
    /// Storage availability changed significantly
    Storage,
    /// Network bandwidth changed significantly
    Network,
    /// GPU devices added or removed
    Gpu,
}

impl ResourceChangeType {
    /// Convert to string for metrics
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Storage => "storage",
            Self::Network => "network",
            Self::Gpu => "gpu",
        }
    }
}

impl NodeCapacity {
    /// Sense current system resources and return a capacity snapshot.
    ///
    /// This function queries the operating system for real resource availability.
    /// On Linux, it reads from /proc filesystem. On other platforms, it falls
    /// back to reasonable defaults.
    ///
    /// # Implementation Status
    ///
    /// - **CPU**: Uses load average from /proc/loadavg to estimate available cores
    /// - **Memory**: Reads MemTotal and MemAvailable from /proc/meminfo
    /// - **Storage**: *Placeholder* - returns 100GB (TODO: use statvfs)
    /// - **Network**: *Placeholder* - returns 1Gbps (TODO: measure bandwidth)
    /// - **GPU**: *Placeholder* - returns empty list (TODO: parse nvidia-smi/ROCm)
    pub fn sense_system_resources() -> Self {
        let now = icn_time::current_timestamp_millis();

        let cpu_total = num_cpus::get() as f64;
        let cpu_available = Self::get_available_cpu_cores(cpu_total);

        Self {
            cpu_cores_total: cpu_total,
            cpu_cores_available: cpu_available,
            memory_mb_total: Self::get_total_memory_mb(),
            memory_mb_available: Self::get_available_memory_mb(),
            // TODO: Implement actual storage sensing via statvfs
            storage_mb_available: 100_000, // Placeholder: 100GB
            // TODO: Implement actual network bandwidth measurement
            network_mbps: 1000.0, // Placeholder: 1Gbps
            gpu_devices: Self::detect_gpus(),
            updated_at: now,
        }
    }

    /// Get available CPU cores based on system load.
    ///
    /// Uses the 1-minute load average from /proc/loadavg to estimate
    /// how many cores are available for new work.
    #[cfg(target_os = "linux")]
    fn get_available_cpu_cores(total_cores: f64) -> f64 {
        std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|content| {
                // /proc/loadavg format: "0.25 0.35 0.30 1/234 12345"
                // First field is 1-minute load average
                content
                    .split_whitespace()
                    .next()
                    .and_then(|load| load.parse::<f64>().ok())
            })
            .map(|load_avg| {
                // Available = total - load, clamped to [0, total]
                (total_cores - load_avg).max(0.0).min(total_cores)
            })
            .unwrap_or(total_cores) // Fallback: assume all cores available
    }

    #[cfg(not(target_os = "linux"))]
    fn get_available_cpu_cores(total_cores: f64) -> f64 {
        // Non-Linux: assume 70% of cores available as heuristic
        total_cores * 0.7
    }

    /// Get total system memory in MB
    #[cfg(target_os = "linux")]
    fn get_total_memory_mb() -> u64 {
        // Read from /proc/meminfo
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("MemTotal:"))
                    .and_then(|line| {
                        line.split_whitespace()
                            .nth(1)
                            .and_then(|kb| kb.parse::<u64>().ok())
                            .map(|kb| kb / 1024) // Convert KB to MB
                    })
            })
            .unwrap_or(8192) // Default 8GB if can't read
    }

    #[cfg(not(target_os = "linux"))]
    fn get_total_memory_mb() -> u64 {
        8192 // Default 8GB for non-Linux systems
    }

    /// Get available system memory in MB
    #[cfg(target_os = "linux")]
    fn get_available_memory_mb() -> u64 {
        // Read from /proc/meminfo
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("MemAvailable:"))
                    .and_then(|line| {
                        line.split_whitespace()
                            .nth(1)
                            .and_then(|kb| kb.parse::<u64>().ok())
                            .map(|kb| kb / 1024) // Convert KB to MB
                    })
            })
            .unwrap_or_else(|| Self::get_total_memory_mb() * 7 / 10) // Default 70% if can't read
    }

    #[cfg(not(target_os = "linux"))]
    fn get_available_memory_mb() -> u64 {
        Self::get_total_memory_mb() * 7 / 10 // Default 70% available
    }

    /// Detect GPU devices
    ///
    /// Placeholder implementation. In a real system, this would:
    /// - Run nvidia-smi for NVIDIA GPUs
    /// - Query ROCm for AMD GPUs
    /// - Parse output and build GpuDevice structs
    fn detect_gpus() -> Vec<GpuDevice> {
        // For now, return empty vec
        // Future: Implement actual GPU detection
        vec![]
    }

    /// Detect significant changes between two capacity snapshots
    ///
    /// Returns a list of resource types that changed significantly
    pub fn detect_changes(&self, other: &NodeCapacity, threshold: f64) -> Vec<ResourceChangeType> {
        let mut changes = Vec::new();

        // Check CPU change (guard against division by zero)
        let cpu_change = if self.cpu_cores_total > 0.0 {
            (self.cpu_cores_available - other.cpu_cores_available).abs() / self.cpu_cores_total
        } else {
            0.0
        };
        if cpu_change >= threshold {
            changes.push(ResourceChangeType::Cpu);
        }

        // Check memory change
        let mem_change = if self.memory_mb_total > 0 {
            (self.memory_mb_available as f64 - other.memory_mb_available as f64).abs()
                / self.memory_mb_total as f64
        } else {
            0.0
        };
        if mem_change >= threshold {
            changes.push(ResourceChangeType::Memory);
        }

        // Check storage change
        // Use the larger value as baseline to ensure stable comparison
        // (e.g., 100GB -> 50GB = 50% change, not 100%)
        let storage_baseline = self.storage_mb_available.max(other.storage_mb_available);
        let storage_change = if storage_baseline > 0 {
            (self.storage_mb_available as f64 - other.storage_mb_available as f64).abs()
                / storage_baseline as f64
        } else {
            0.0
        };
        if storage_change >= threshold {
            changes.push(ResourceChangeType::Storage);
        }

        // Check network bandwidth change
        let net_change = if self.network_mbps > 0.0 {
            (self.network_mbps - other.network_mbps).abs() / self.network_mbps
        } else {
            0.0
        };
        if net_change >= threshold {
            changes.push(ResourceChangeType::Network);
        }

        // Check GPU changes (devices added/removed)
        if self.gpu_devices.len() != other.gpu_devices.len() {
            changes.push(ResourceChangeType::Gpu);
        } else {
            // Check if availability changed for any GPU
            for (gpu_self, gpu_other) in self.gpu_devices.iter().zip(other.gpu_devices.iter()) {
                if gpu_self.available != gpu_other.available {
                    changes.push(ResourceChangeType::Gpu);
                    break;
                }
            }
        }

        changes
    }
}

// ============================================================================
// Capacity Budget (scope-aware resource allocation)
// ============================================================================

/// Per-scope resource allocation fractions.
///
/// Defines what fraction of a node's total capacity is available to each
/// scope level. The kernel uses these fractions for admission control without
/// interpreting the organizational semantics behind scopes.
///
/// Fractions must sum to ≤ 1.0 (unused fraction is unallocated headroom).
///
/// # Example
///
/// ```
/// use icn_compute::CapacityBudget;
/// use icn_kernel_api::ScopeLevel;
///
/// let budget = CapacityBudget::default();
/// assert_eq!(budget.fraction_for(ScopeLevel::Local), 0.30);
/// assert_eq!(budget.fraction_for(ScopeLevel::Cell), 0.25);
/// assert!(budget.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapacityBudget {
    /// Fraction reserved for local tasks (default: 0.30)
    pub local_reserve: f64,

    /// Fraction shared with cell peers (default: 0.25)
    pub cell_share: f64,

    /// Fraction shared with org-wide tasks (default: 0.20)
    pub org_share: f64,

    /// Fraction shared with federation tasks (default: 0.15)
    pub federation_share: f64,

    /// Fraction shared with commons tasks (default: 0.10)
    pub commons_share: f64,
}

impl Default for CapacityBudget {
    fn default() -> Self {
        Self {
            local_reserve: 0.30,
            cell_share: 0.25,
            org_share: 0.20,
            federation_share: 0.15,
            commons_share: 0.10,
        }
    }
}

impl CapacityBudget {
    /// Get the capacity fraction for a given scope level.
    ///
    /// Returns the fraction of total node capacity available to tasks
    /// at the specified scope.
    pub fn fraction_for(&self, scope: ScopeLevel) -> f64 {
        match scope {
            ScopeLevel::Local => self.local_reserve,
            ScopeLevel::Cell => self.cell_share,
            ScopeLevel::Org => self.org_share,
            ScopeLevel::Federation => self.federation_share,
            ScopeLevel::Commons => self.commons_share,
        }
    }

    /// Get the cumulative capacity available at a given scope and below.
    ///
    /// Tasks at a wider scope can use capacity from all narrower scopes
    /// that haven't been consumed. This returns the maximum fraction
    /// available if the scope widened from Local up to the given level.
    pub fn cumulative_fraction_up_to(&self, scope: ScopeLevel) -> f64 {
        let mut total = self.local_reserve;
        if scope >= ScopeLevel::Cell {
            total += self.cell_share;
        }
        if scope >= ScopeLevel::Org {
            total += self.org_share;
        }
        if scope >= ScopeLevel::Federation {
            total += self.federation_share;
        }
        if scope >= ScopeLevel::Commons {
            total += self.commons_share;
        }
        total
    }

    /// Validate that fractions are individually valid and sum to ≤ 1.0.
    pub fn validate(&self) -> Result<(), &'static str> {
        let fractions = [
            self.local_reserve,
            self.cell_share,
            self.org_share,
            self.federation_share,
            self.commons_share,
        ];

        for &f in &fractions {
            if !(0.0..=1.0).contains(&f) {
                return Err("Each fraction must be in [0.0, 1.0]");
            }
        }

        let sum: f64 = fractions.iter().sum();
        if sum > 1.0 + Self::FLOAT_TOLERANCE {
            return Err("Fractions must sum to at most 1.0");
        }

        Ok(())
    }

    /// Practical tolerance for floating-point comparisons.
    ///
    /// Using `f64::EPSILON` (~2.2e-16) is too tight when fractions are
    /// adjusted over many rounds; accumulated rounding can exceed it.
    /// 1e-9 is well above rounding noise but invisible at the 0.01 scale
    /// of our smallest meaningful fraction.
    const FLOAT_TOLERANCE: f64 = 1e-9;

    /// Minimum fraction per scope during demand adjustment.
    ///
    /// Prevents complete starvation of any scope, ensuring every scope
    /// retains at least minimal capacity for burst traffic.
    ///
    /// **Security**: Without this bound, an attacker could submit sustained
    /// high-demand tasks in one scope to drive other scopes' budgets to zero
    /// via the demand feedback loop, effectively denying service to those scopes.
    const MIN_SCOPE_FRACTION: f64 = 0.01;

    /// Maximum fraction per scope during demand adjustment.
    ///
    /// Prevents any single scope from monopolizing capacity, leaving
    /// headroom for other scopes even under sustained high demand.
    ///
    /// **Security**: Without this bound, a single dominant scope could absorb
    /// nearly all capacity, leaving other scopes unable to schedule tasks
    /// even when legitimate demand arises.
    const MAX_SCOPE_FRACTION: f64 = 0.80;

    /// Adjust budget based on observed demand.
    ///
    /// Takes utilization ratios (0.0-1.0) per scope and shifts unused
    /// capacity toward scopes with higher demand. The adjustment is
    /// conservative (moves at most `learning_rate` fraction per call).
    ///
    /// # Arguments
    /// * `utilization` - Map of scope → utilization ratio (0.0 = idle, 1.0 = saturated).
    ///   Scopes absent from the map are left unchanged (only scopes with data
    ///   participate in the rebalancing).
    /// * `learning_rate` - Maximum fraction to shift per adjustment (clamped to 0.0..0.1)
    ///
    /// # Bounds
    ///
    /// Fractions are clamped to [`MIN_SCOPE_FRACTION`, `MAX_SCOPE_FRACTION`]
    /// (currently \[0.01, 0.80\]) to prevent starvation (min) and monopolization
    /// (max), ensuring every scope retains headroom even under sustained
    /// extreme demand patterns. After clamping, fractions are re-normalized
    /// to preserve the original sum.
    pub fn adjust_from_demand(
        &mut self,
        utilization: &HashMap<ScopeLevel, f64>,
        learning_rate: f64,
    ) {
        let lr = learning_rate.clamp(0.0, 0.1);

        let scopes = ScopeLevel::ALL;
        let mut fractions = [
            self.local_reserve,
            self.cell_share,
            self.org_share,
            self.federation_share,
            self.commons_share,
        ];

        // Calculate average utilization
        let mut total_util = 0.0;
        let mut count = 0usize;
        for &scope in &scopes {
            if let Some(&u) = utilization.get(&scope) {
                total_util += u;
                count += 1;
            }
        }
        let avg_util = if count > 0 {
            total_util / count as f64
        } else {
            tracing::trace!("adjust_from_demand: no utilization data, skipping adjustment");
            return;
        };

        // Shift capacity: over-utilized scopes gain, under-utilized lose
        let mut deltas = [0.0f64; 5];
        for (i, &scope) in scopes.iter().enumerate() {
            if let Some(&u) = utilization.get(&scope) {
                let diff = u - avg_util;
                deltas[i] = diff * lr;
            }
        }

        // Apply deltas, clamping to prevent starvation and monopolization
        for i in 0..5 {
            fractions[i] = (fractions[i] + deltas[i])
                .clamp(Self::MIN_SCOPE_FRACTION, Self::MAX_SCOPE_FRACTION);
        }

        // Normalize so sum stays the same as before
        let original_sum: f64 = [
            self.local_reserve,
            self.cell_share,
            self.org_share,
            self.federation_share,
            self.commons_share,
        ]
        .iter()
        .sum();
        let new_sum: f64 = fractions.iter().sum();
        if new_sum > Self::FLOAT_TOLERANCE {
            let scale = original_sum / new_sum;
            for f in &mut fractions {
                *f = (*f * scale).clamp(0.0, 1.0);
            }
        }

        self.local_reserve = fractions[0];
        self.cell_share = fractions[1];
        self.org_share = fractions[2];
        self.federation_share = fractions[3];
        self.commons_share = fractions[4];
    }
}

/// Configuration for the demand-feedback capacity adjustment loop.
///
/// Controls how frequently and aggressively the per-scope capacity budget
/// is rebalanced based on observed queue depths.
#[derive(Debug, Clone)]
pub struct DemandAdjustmentConfig {
    /// Interval between adjustment rounds in seconds (default: 60).
    pub interval_secs: u64,

    /// Maximum fraction shifted per adjustment round (default: 0.05).
    /// Clamped to [0.0, 0.1] by `CapacityBudget::adjust_from_demand`.
    pub learning_rate: f64,

    /// Minimum number of queued tasks across all scopes before adjustment
    /// kicks in (default: 5). Prevents noisy adjustments from sparse data.
    pub min_samples: usize,
}

impl Default for DemandAdjustmentConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            learning_rate: 0.05,
            min_samples: 5,
        }
    }
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

/// Federation placement policy for cross-cooperative task execution.
///
/// Controls whether and how tasks can be placed on executors from
/// federated cooperatives. This is evaluated during placement scoring.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum FederationPolicy {
    /// Prefer local executors, only use federated if no local available.
    /// Local executors get a score bonus (configurable).
    #[default]
    PreferLocal,

    /// Allow federated executors with equal consideration.
    /// No score adjustment based on federation status.
    AllowFederated,

    /// Only use executors from whitelisted cooperatives.
    /// Tasks will only be placed on executors from listed coops.
    FederatedWhitelist {
        /// Cooperative IDs that are allowed to execute this task
        cooperatives: Vec<String>,
    },

    /// Block all federated executors - local only.
    /// Tasks can only be executed by same-cooperative executors.
    LocalOnly,
}

/// Extended placement constraints with federation support.
///
/// Embeds base `PlacementConstraints` from the policy module and adds
/// federation-specific controls for cross-cooperative task execution.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FederatedPlacementConstraints {
    /// Base placement constraints (region, whitelist, blacklist, capabilities)
    #[serde(flatten)]
    pub base: crate::policy::PlacementConstraints,

    /// Federation placement policy
    pub federation_policy: FederationPolicy,

    /// Minimum required federated trust score (0.0-1.0)
    /// Federated executors below this threshold are rejected.
    /// Default: 0.3 (same as MIN_TRUST_EXECUTE)
    pub min_federated_trust: Option<f64>,

    /// Local preference weight (0.0-1.0) when using PreferLocal policy.
    /// Higher values give more advantage to local executors.
    /// Default: 0.15 (15% score bonus for local executors)
    pub local_preference_weight: Option<f64>,
}

impl FederatedPlacementConstraints {
    /// Default minimum federated trust threshold
    pub const DEFAULT_MIN_FEDERATED_TRUST: f64 = 0.3;

    /// Default local preference weight
    pub const DEFAULT_LOCAL_PREFERENCE_WEIGHT: f64 = 0.15;

    /// Check if an executor from a given cooperative is allowed by this policy
    pub fn allows_cooperative(&self, coop_id: Option<&str>, is_local: bool) -> bool {
        match &self.federation_policy {
            FederationPolicy::LocalOnly => is_local,
            FederationPolicy::PreferLocal | FederationPolicy::AllowFederated => true,
            FederationPolicy::FederatedWhitelist { cooperatives } => {
                if is_local {
                    true // Local is always allowed
                } else {
                    coop_id.is_some_and(|id| cooperatives.iter().any(|c| c == id))
                }
            }
        }
    }

    /// Get the effective minimum federated trust threshold
    pub fn effective_min_trust(&self) -> f64 {
        self.min_federated_trust
            .unwrap_or(Self::DEFAULT_MIN_FEDERATED_TRUST)
    }

    /// Get the effective local preference weight
    pub fn effective_local_preference(&self) -> f64 {
        self.local_preference_weight
            .unwrap_or(Self::DEFAULT_LOCAL_PREFERENCE_WEIGHT)
    }
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

    // -- Scope-aware placement fields (Epic 2) --
    /// Maximum scope level the task is allowed to execute at.
    ///
    /// If `None`, no scope restriction (equivalent to `Commons`).
    /// If set to e.g. `ScopeLevel::Org`, the task will not be placed
    /// on nodes outside the submitter's organization.
    #[serde(default)]
    pub max_scope: Option<ScopeLevel>,

    /// Preferred cell for execution (e.g., submitter's own cell).
    ///
    /// Executors in this cell receive a placement score bonus.
    /// If `None`, no cell affinity preference.
    #[serde(default)]
    pub cell_affinity: Option<CellId>,

    /// Explicit list of allowed scope levels for execution.
    ///
    /// If empty, all scopes are accepted (no restriction).
    /// If non-empty, only executors whose scope is in this list may run the task.
    /// This is more expressive than `max_scope`: e.g., allow `Cell` and `Federation`
    /// but not `Org`.
    #[serde(default)]
    pub allowed_scopes: Vec<ScopeLevel>,
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

impl Eq for PlacementOffer {}

impl Ord for PlacementOffer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Use total_cmp for deterministic ordering even with NaN values
        // (NaN is considered greater than all other values per IEEE 754 total ordering)
        // Compare all fields for consistency with derived PartialEq
        self.score
            .total_cmp(&other.score)
            .then_with(|| self.cost.cmp(&other.cost))
            .then_with(|| self.estimated_start.cmp(&other.estimated_start))
            .then_with(|| self.offered_at.cmp(&other.offered_at))
            .then_with(|| self.executor.cmp(&other.executor))
    }
}

impl PartialOrd for PlacementOffer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Scope context for placement scoring.
///
/// Provides the relationship between a submitter and an executor
/// so the placement policy can apply scope-aware score adjustments.
#[derive(Debug, Clone)]
pub struct ScopeContext {
    /// The scope relationship between submitter and executor.
    ///
    /// - `Local` = same node
    /// - `Cell` = same cell
    /// - `Org` = same organization, different cell
    /// - `Federation` = federated peer
    /// - `Commons` = unknown / unaffiliated
    pub peer_scope: ScopeLevel,

    /// The executor's cell (if any)
    pub executor_cell: Option<CellId>,

    /// Capacity budget for scope-aware admission control
    pub capacity_budget: CapacityBudget,
}

impl ScopeContext {
    /// Create an empty scope context (no scope information).
    ///
    /// Defaults to `Commons` scope with default budget.
    pub fn empty() -> Self {
        Self {
            peer_scope: ScopeLevel::Commons,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        }
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

    /// Per-scope queue depths for scope-aware load balancing.
    ///
    /// Tracks how many pending tasks originated from each scope level,
    /// allowing the scheduler to respect per-scope capacity budgets.
    pub scope_queue_depths: HashMap<ScopeLevel, usize>,
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
    /// # Parameters
    ///
    /// - `request`: The full placement request (task_hash, resource_profile,
    ///   locality_hints, max_scope, cell_affinity)
    /// - `submitter`: DID of the task submitter
    /// - `node_state`: Current executor node capacity and queue state
    /// - `trust_score`: Submitter's trust score as seen by the executor
    /// - `locality_ctx`: Network context for computing locality scores (RTT, blob locations)
    /// - `scope_ctx`: Scope relationship between submitter and executor for scope-aware scoring
    fn score_task(
        &self,
        request: &PlacementRequest,
        submitter: &str,
        node_state: &NodeState,
        trust_score: f64,
        locality_ctx: &LocalityContext,
        scope_ctx: &ScopeContext,
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

impl DefaultPlacementPolicy {
    /// Default assumed CPU cores per task when the profile doesn't specify.
    ///
    /// Used for scope budget estimation (queue depth limit calculation).
    const DEFAULT_CPU_PER_TASK: f64 = 0.1;

    /// Placement affinity bonus for a given scope level.
    ///
    /// Narrower scope = stronger affinity, rewarding local execution.
    /// These values are additive components of the overall placement score.
    fn scope_affinity_bonus(scope: ScopeLevel) -> f64 {
        match scope {
            ScopeLevel::Local => 0.15,
            ScopeLevel::Cell => 0.12,
            ScopeLevel::Org => 0.08,
            ScopeLevel::Federation => 0.04,
            ScopeLevel::Commons => 0.00,
        }
    }
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
        request: &PlacementRequest,
        _submitter: &str,
        node_state: &NodeState,
        trust_score: f64,
        locality_ctx: &LocalityContext,
        scope_ctx: &ScopeContext,
    ) -> Option<PlacementOffer> {
        let profile = &request.resource_profile;
        // Trust gate
        if trust_score < self.min_trust {
            return None;
        }

        // Scope gate: reject if the task's max_scope restricts us
        // and the submitter-executor relationship exceeds it
        if let Some(max_scope) = request.max_scope {
            if scope_ctx.peer_scope > max_scope {
                return None; // Executor is too far from submitter
            }
        }

        // Allowed scopes gate: reject if executor's scope is not in the allowed list
        if !request.allowed_scopes.is_empty()
            && !request.allowed_scopes.contains(&scope_ctx.peer_scope)
        {
            return None; // Executor's scope not in allowed list
        }

        // Capacity check
        if !node_state.capacity.can_fit(profile) {
            return None;
        }

        // Scope-aware capacity check: ensure scope budget isn't exhausted.
        //
        // We estimate the max number of tasks allowed per scope based on total
        // CPU cores and the scope's budget fraction. Uses a conservative estimate
        // of CPU per task (the profile's requested cores, or a default if unset).
        let scope_queue = node_state
            .scope_queue_depths
            .get(&scope_ctx.peer_scope)
            .copied()
            .unwrap_or(0);
        let scope_fraction = scope_ctx.capacity_budget.fraction_for(scope_ctx.peer_scope);
        let cpu_per_task = profile
            .cpu_cores
            .unwrap_or(DefaultPlacementPolicy::DEFAULT_CPU_PER_TASK);
        // Calculate max concurrent tasks this scope can run:
        // (total_cpu * scope_fraction) gives available CPU for this scope,
        // dividing by cpu_per_task gives the task slot capacity.
        let available_cpu_for_scope = node_state.capacity.cpu_cores_total * scope_fraction;
        let total_queue_budget =
            if available_cpu_for_scope > f64::EPSILON && cpu_per_task > f64::EPSILON {
                (available_cpu_for_scope / cpu_per_task).max(1.0) as usize
            } else {
                0
            };
        if total_queue_budget > 0 && scope_queue >= total_queue_budget {
            return None; // Scope budget exhausted
        }

        // Compute score (0.0 - 1.0)
        let mut score = 0.0;

        // Factor 1: Trust (weight 0.20)
        score += (trust_score * 0.20).min(0.20);

        // Factor 2: Available capacity (weight 0.15)
        let capacity_ratio = node_state.capacity.available_ratio();
        score += capacity_ratio * 0.15;

        // Factor 3: Queue depth (weight 0.10)
        let queue_penalty = (node_state.queue_depth as f64 / 10.0).min(1.0);
        score += (1.0 - queue_penalty) * 0.10;

        // Factor 4: Network latency (weight 0.10)
        if let Some(rtt_ms) = locality_ctx.submitter_rtt_ms {
            let rtt_score = (1.0 - (rtt_ms as f64 / 500.0).min(1.0)).max(0.0);
            score += rtt_score * 0.10;
        }

        // Factor 5: Data locality (weight 0.10)
        let data_locality_ratio = locality_ctx.data_locality_ratio();
        score += data_locality_ratio * 0.10;

        // Factor 6: Scope affinity (weight 0.15, NEW)
        // Narrower scope = stronger affinity bonus
        score += Self::scope_affinity_bonus(scope_ctx.peer_scope);

        // Factor 7: Cell affinity bonus (weight 0.05)
        // Extra bonus if executor is in the preferred cell
        if let (Some(ref preferred), Some(ref executor_cell)) =
            (&request.cell_affinity, &scope_ctx.executor_cell)
        {
            if preferred == executor_cell {
                score += 0.05;
            }
        }

        // Factor 8: Locality hints bonus (weight 0.05)
        let mut hint_bonus: f64 = 0.0;
        for hint in &request.locality_hints {
            match hint {
                LocalityHint::PreferRegion(region) => {
                    if let Some(ref own_region) = locality_ctx.own_region {
                        if own_region == region {
                            hint_bonus += 0.03;
                        }
                    }
                }
                LocalityHint::PreferDid(did) => {
                    if &node_state.did == did {
                        hint_bonus += 0.05;
                    }
                }
                LocalityHint::AvoidDid(did) => {
                    if &node_state.did == did {
                        return None;
                    }
                }
                _ => {}
            }
        }
        score += hint_bonus.min(0.05);

        // Factor 9: Random jitter (weight 0.10)
        use rand::Rng;
        let jitter = rand::thread_rng().gen::<f64>() * 0.10;
        score += jitter;

        // Cap final score at 1.0
        // Max theoretical: 0.20 + 0.15 + 0.10 + 0.10 + 0.10 + 0.15 + 0.05 + 0.05 + 0.10 = 1.00
        score = score.min(1.0);

        // Calculate cost
        let load_factor = if node_state.queue_depth > 5 {
            self.load_multiplier
        } else {
            1.0
        };
        let cost = (self.base_cost as f64 * load_factor) as u64;

        let estimated_start = icn_time::current_timestamp_millis() + node_state.queue_depth_ms();

        Some(PlacementOffer {
            executor: node_state.did.clone(),
            score,
            cost,
            estimated_start,
            offered_at: icn_time::current_timestamp_millis(),
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
            scope_queue_depths: HashMap::new(),
        };

        let task_hash = [0u8; 32];
        let empty_locality = LocalityContext::empty();

        let scope_ctx = ScopeContext::empty();
        let request = PlacementRequest {
            task_hash,
            resource_profile: profile.clone(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: None,
            cell_affinity: None,
            allowed_scopes: vec![],
        };

        // High trust, should get good score
        let offer = policy
            .score_task(
                &request,
                "did:icn:alice",
                &node_state,
                0.8,
                &empty_locality,
                &scope_ctx,
            )
            .unwrap();

        assert!(offer.score > 0.3); // Trust + capacity + queue + jitter
        assert_eq!(offer.cost, 10); // Base cost

        // Low trust, rejected
        let offer = policy.score_task(
            &request,
            "did:icn:untrusted",
            &node_state,
            0.1,
            &empty_locality,
            &scope_ctx,
        );
        assert!(offer.is_none());

        // High queue, cost multiplier kicks in
        let busy_node = NodeState {
            queue_depth: 10,
            ..node_state.clone()
        };

        let offer = policy
            .score_task(
                &request,
                "did:icn:alice",
                &busy_node,
                0.8,
                &empty_locality,
                &scope_ctx,
            )
            .unwrap();

        assert_eq!(offer.cost, 15); // base_cost * load_multiplier

        // Test scope gate: reject if max_scope is exceeded
        let local_only_request = PlacementRequest {
            max_scope: Some(ScopeLevel::Cell),
            ..request.clone()
        };
        // scope_ctx has peer_scope=Commons which > Cell, should be rejected
        let offer = policy.score_task(
            &local_only_request,
            "did:icn:alice",
            &node_state,
            0.8,
            &empty_locality,
            &scope_ctx,
        );
        assert!(offer.is_none(), "Should reject when peer_scope > max_scope");

        // Same-cell context should get scope bonus
        let cell_ctx = ScopeContext {
            peer_scope: ScopeLevel::Cell,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        let cell_offer = policy
            .score_task(
                &request,
                "did:icn:alice",
                &node_state,
                0.8,
                &empty_locality,
                &cell_ctx,
            )
            .unwrap();
        // Cell scope bonus (0.12) vs Commons (0.00) = +0.12 advantage
        // May vary by jitter, but on average cell should score higher
        assert!(cell_offer.score > 0.3);
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

    #[test]
    fn test_placement_offer_nan_handling() {
        // Test that NaN scores are handled deterministically without panics
        let offer_nan = PlacementOffer {
            executor: "did:icn:nan".into(),
            score: f64::NAN,
            cost: 100,
            estimated_start: 1000,
            offered_at: 1000,
        };

        let offer_valid = PlacementOffer {
            executor: "did:icn:valid".into(),
            score: 0.5,
            cost: 100,
            estimated_start: 1000,
            offered_at: 1000,
        };

        let offer_neg_inf = PlacementOffer {
            executor: "did:icn:neg_inf".into(),
            score: f64::NEG_INFINITY,
            cost: 100,
            estimated_start: 1000,
            offered_at: 1000,
        };

        // NaN should be greater than all other values (per IEEE 754 total ordering)
        assert!(offer_nan > offer_valid);
        assert!(offer_nan > offer_neg_inf);
        assert!(offer_neg_inf < offer_valid);

        // Sorting should not panic and should be deterministic
        let mut offers = [
            offer_valid.clone(),
            offer_nan.clone(),
            offer_neg_inf.clone(),
        ];
        offers.sort();

        // After sorting: -Inf < 0.5 < NaN
        assert_eq!(offers[0].score, f64::NEG_INFINITY);
        assert_eq!(offers[1].score, 0.5);
        assert!(offers[2].score.is_nan());

        // Test Eq/Ord consistency: two offers with same fields should be equal
        let offer_copy = offer_valid.clone();
        assert_eq!(offer_valid, offer_copy);
        assert!(offer_valid.cmp(&offer_copy) == std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_resource_refresh_config() {
        // Test default config
        let config = ResourceRefreshConfig::default();
        assert_eq!(config.refresh_interval_secs, 300); // 5 minutes
        assert_eq!(config.change_threshold, 0.1); // 10%

        // Test with custom interval
        let config = ResourceRefreshConfig::with_interval(60);
        assert_eq!(config.refresh_interval_secs, 60);
        assert_eq!(config.change_threshold, 0.1);

        // Test with custom threshold
        let config = ResourceRefreshConfig::with_threshold(0.05);
        assert_eq!(config.refresh_interval_secs, 300);
        assert_eq!(config.change_threshold, 0.05);
    }

    #[test]
    fn test_resource_change_type_as_str() {
        assert_eq!(ResourceChangeType::Cpu.as_str(), "cpu");
        assert_eq!(ResourceChangeType::Memory.as_str(), "memory");
        assert_eq!(ResourceChangeType::Storage.as_str(), "storage");
        assert_eq!(ResourceChangeType::Network.as_str(), "network");
        assert_eq!(ResourceChangeType::Gpu.as_str(), "gpu");
    }

    #[test]
    fn test_detect_changes_no_change() {
        let capacity1 = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let capacity2 = capacity1.clone();

        let changes = capacity1.detect_changes(&capacity2, 0.1);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_detect_changes_cpu() {
        let capacity1 = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let mut capacity2 = capacity1.clone();
        capacity2.cpu_cores_available = 7.0; // 3.0 / 8.0 = 37.5% change

        let changes = capacity1.detect_changes(&capacity2, 0.1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], ResourceChangeType::Cpu);
    }

    #[test]
    fn test_detect_changes_memory() {
        let capacity1 = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let mut capacity2 = capacity1.clone();
        capacity2.memory_mb_available = 6000; // 2192 / 16384 = 13.4% change

        let changes = capacity1.detect_changes(&capacity2, 0.1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], ResourceChangeType::Memory);
    }

    #[test]
    fn test_detect_changes_gpu_added() {
        let capacity1 = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let mut capacity2 = capacity1.clone();
        capacity2.gpu_devices.push(GpuDevice {
            device_id: "0".to_string(),
            memory_gb: 16,
            compute_capability: "sm_80".to_string(),
            device_name: "NVIDIA A100".to_string(),
            available: true,
        });

        let changes = capacity1.detect_changes(&capacity2, 0.1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], ResourceChangeType::Gpu);
    }

    #[test]
    fn test_detect_changes_multiple() {
        let capacity1 = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let mut capacity2 = capacity1.clone();
        capacity2.cpu_cores_available = 7.0; // CPU changed
        capacity2.memory_mb_available = 6000; // Memory changed

        let changes = capacity1.detect_changes(&capacity2, 0.1);
        assert_eq!(changes.len(), 2);
        assert!(changes.contains(&ResourceChangeType::Cpu));
        assert!(changes.contains(&ResourceChangeType::Memory));
    }

    #[test]
    fn test_detect_changes_below_threshold() {
        let capacity1 = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 4.0,
            memory_mb_total: 16384,
            memory_mb_available: 8192,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        };

        let mut capacity2 = capacity1.clone();
        capacity2.cpu_cores_available = 4.5; // 0.5 / 8.0 = 6.25% change (below 10% threshold)

        let changes = capacity1.detect_changes(&capacity2, 0.1);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_sense_system_resources() {
        // Just verify it doesn't panic and returns reasonable values
        let capacity = NodeCapacity::sense_system_resources();

        assert!(capacity.cpu_cores_total > 0.0);
        // cpu_cores_available can be 0 when system is under heavy load (load avg >= cores)
        // This is valid behavior, not a test failure
        assert!(capacity.cpu_cores_available >= 0.0);
        assert!(capacity.memory_mb_total > 0);
        assert!(capacity.memory_mb_available > 0);
        assert!(capacity.updated_at > 0);
    }

    // ================================================================
    // Epic 2: Scope-aware capacity and placement tests
    // ================================================================

    #[test]
    fn test_capacity_budget_default() {
        let budget = CapacityBudget::default();
        assert_eq!(budget.local_reserve, 0.30);
        assert_eq!(budget.cell_share, 0.25);
        assert_eq!(budget.org_share, 0.20);
        assert_eq!(budget.federation_share, 0.15);
        assert_eq!(budget.commons_share, 0.10);
        assert!(budget.validate().is_ok());
    }

    #[test]
    fn test_capacity_budget_fraction_for() {
        let budget = CapacityBudget::default();
        assert_eq!(budget.fraction_for(ScopeLevel::Local), 0.30);
        assert_eq!(budget.fraction_for(ScopeLevel::Cell), 0.25);
        assert_eq!(budget.fraction_for(ScopeLevel::Org), 0.20);
        assert_eq!(budget.fraction_for(ScopeLevel::Federation), 0.15);
        assert_eq!(budget.fraction_for(ScopeLevel::Commons), 0.10);
    }

    #[test]
    fn test_capacity_budget_cumulative() {
        let budget = CapacityBudget::default();
        assert_eq!(budget.cumulative_fraction_up_to(ScopeLevel::Local), 0.30);
        assert!((budget.cumulative_fraction_up_to(ScopeLevel::Cell) - 0.55).abs() < f64::EPSILON);
        assert!(
            (budget.cumulative_fraction_up_to(ScopeLevel::Commons) - 1.00).abs() < f64::EPSILON
        );
    }

    #[test]
    fn test_capacity_budget_validation() {
        // Valid: sums to 1.0
        let budget = CapacityBudget::default();
        assert!(budget.validate().is_ok());

        // Valid: sums to < 1.0 (headroom)
        let budget = CapacityBudget {
            local_reserve: 0.20,
            cell_share: 0.20,
            org_share: 0.20,
            federation_share: 0.10,
            commons_share: 0.10,
        };
        assert!(budget.validate().is_ok());

        // Invalid: negative fraction
        let budget = CapacityBudget {
            local_reserve: -0.1,
            ..Default::default()
        };
        assert!(budget.validate().is_err());

        // Invalid: fraction > 1.0
        let budget = CapacityBudget {
            local_reserve: 1.1,
            ..Default::default()
        };
        assert!(budget.validate().is_err());

        // Invalid: sum > 1.0
        let budget = CapacityBudget {
            local_reserve: 0.50,
            cell_share: 0.50,
            org_share: 0.20,
            federation_share: 0.0,
            commons_share: 0.0,
        };
        assert!(budget.validate().is_err());
    }

    #[test]
    fn test_capacity_budget_demand_adjustment() {
        let mut budget = CapacityBudget::default();
        let original_sum: f64 = [
            budget.local_reserve,
            budget.cell_share,
            budget.org_share,
            budget.federation_share,
            budget.commons_share,
        ]
        .iter()
        .sum();

        // Simulate high local demand, low commons demand
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Local, 0.95);
        utilization.insert(ScopeLevel::Cell, 0.50);
        utilization.insert(ScopeLevel::Org, 0.30);
        utilization.insert(ScopeLevel::Federation, 0.20);
        utilization.insert(ScopeLevel::Commons, 0.05);

        budget.adjust_from_demand(&utilization, 0.05);

        // Local should have grown, commons should have shrunk
        assert!(budget.local_reserve > 0.30);
        assert!(budget.commons_share < 0.10);

        // Sum should be preserved
        let new_sum: f64 = [
            budget.local_reserve,
            budget.cell_share,
            budget.org_share,
            budget.federation_share,
            budget.commons_share,
        ]
        .iter()
        .sum();
        assert!((new_sum - original_sum).abs() < 0.001);
    }

    #[test]
    fn test_scope_context_empty() {
        let ctx = ScopeContext::empty();
        assert_eq!(ctx.peer_scope, ScopeLevel::Commons);
        assert!(ctx.executor_cell.is_none());
    }

    #[test]
    fn test_placement_request_scope_fields_default() {
        // Backward compatibility: new fields default to None
        let request: PlacementRequest = serde_json::from_str(
            r#"{
                "task_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                "resource_profile": {"cpu_cores": 0.1, "memory_mb": 128},
                "locality_hints": [],
                "max_cost": null,
                "requested_at": 1000
            }"#,
        )
        .unwrap();
        assert!(request.max_scope.is_none());
        assert!(request.cell_affinity.is_none());
        assert!(request.allowed_scopes.is_empty());
    }

    #[test]
    fn test_scope_gate_rejects_out_of_scope() {
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
            queue_depth: 0,
            scope_queue_depths: HashMap::new(),
        };

        let task_hash = [0u8; 32];
        let empty_locality = LocalityContext::empty();

        // Request restricted to Cell scope
        let request = PlacementRequest {
            task_hash,
            resource_profile: profile.clone(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: Some(ScopeLevel::Cell),
            cell_affinity: None,
            allowed_scopes: vec![],
        };

        // Executor at Commons scope (too far) → rejected
        let commons_ctx = ScopeContext {
            peer_scope: ScopeLevel::Commons,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        assert!(policy
            .score_task(
                &request,
                "did:icn:alice",
                &node_state,
                0.8,
                &empty_locality,
                &commons_ctx,
            )
            .is_none());

        // Executor at Cell scope (within range) → accepted
        let cell_ctx = ScopeContext {
            peer_scope: ScopeLevel::Cell,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        assert!(policy
            .score_task(
                &request,
                "did:icn:alice",
                &node_state,
                0.8,
                &empty_locality,
                &cell_ctx,
            )
            .is_some());

        // Executor at Local scope (narrower than max) → accepted
        let local_ctx = ScopeContext {
            peer_scope: ScopeLevel::Local,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        assert!(policy
            .score_task(
                &request,
                "did:icn:alice",
                &node_state,
                0.8,
                &empty_locality,
                &local_ctx,
            )
            .is_some());
    }

    #[test]
    fn test_scope_affinity_scoring() {
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
            queue_depth: 0,
            scope_queue_depths: HashMap::new(),
        };

        let task_hash = [0u8; 32];
        let empty_locality = LocalityContext::empty();

        let request = PlacementRequest {
            task_hash,
            resource_profile: profile.clone(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: None,
            cell_affinity: None,
            allowed_scopes: vec![],
        };

        // Collect scores from many runs to smooth out jitter
        let n = 100;
        let mut local_total = 0.0;
        let mut commons_total = 0.0;

        for _ in 0..n {
            let local_ctx = ScopeContext {
                peer_scope: ScopeLevel::Local,
                executor_cell: None,
                capacity_budget: CapacityBudget::default(),
            };
            let commons_ctx = ScopeContext::empty();

            local_total += policy
                .score_task(
                    &request,
                    "did:icn:alice",
                    &node_state,
                    0.8,
                    &empty_locality,
                    &local_ctx,
                )
                .unwrap()
                .score;

            commons_total += policy
                .score_task(
                    &request,
                    "did:icn:alice",
                    &node_state,
                    0.8,
                    &empty_locality,
                    &commons_ctx,
                )
                .unwrap()
                .score;
        }

        let local_avg = local_total / n as f64;
        let commons_avg = commons_total / n as f64;

        // Local scope should consistently score higher than Commons
        // due to the 0.15 scope affinity bonus
        assert!(
            local_avg > commons_avg + 0.10,
            "Local avg ({local_avg:.4}) should be > Commons avg ({commons_avg:.4}) + 0.10"
        );
    }

    #[test]
    fn test_demand_adjustment_all_saturated() {
        let mut budget = CapacityBudget::default();
        let original = budget.clone();

        // All scopes at 100% utilization — all equal, so no shift
        let mut utilization = HashMap::new();
        for &scope in &ScopeLevel::ALL {
            utilization.insert(scope, 1.0);
        }

        budget.adjust_from_demand(&utilization, 0.05);

        // Fractions should remain approximately unchanged (all at same util)
        assert!((budget.local_reserve - original.local_reserve).abs() < 0.001);
        assert!((budget.commons_share - original.commons_share).abs() < 0.001);
    }

    #[test]
    fn test_demand_adjustment_one_hot() {
        let mut budget = CapacityBudget::default();

        // Only Local at 100%, rest at 0%
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Local, 1.0);
        utilization.insert(ScopeLevel::Cell, 0.0);
        utilization.insert(ScopeLevel::Org, 0.0);
        utilization.insert(ScopeLevel::Federation, 0.0);
        utilization.insert(ScopeLevel::Commons, 0.0);

        budget.adjust_from_demand(&utilization, 0.05);

        // Local should have gained, others should have shrunk
        assert!(budget.local_reserve > 0.30);
        assert!(budget.cell_share < 0.25);
        assert!(budget.org_share < 0.20);
        assert!(budget.federation_share < 0.15);
        assert!(budget.commons_share < 0.10);

        // All fractions still within bounds
        assert!(budget.validate().is_ok());
    }

    #[test]
    fn test_demand_adjustment_convergence() {
        let mut budget = CapacityBudget::default();

        // Simulate 50 rounds of adjustment with stable demand pattern
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Local, 0.90);
        utilization.insert(ScopeLevel::Cell, 0.70);
        utilization.insert(ScopeLevel::Org, 0.30);
        utilization.insert(ScopeLevel::Federation, 0.10);
        utilization.insert(ScopeLevel::Commons, 0.05);

        let original_sum: f64 = [
            budget.local_reserve,
            budget.cell_share,
            budget.org_share,
            budget.federation_share,
            budget.commons_share,
        ]
        .iter()
        .sum();

        for _ in 0..50 {
            budget.adjust_from_demand(&utilization, 0.05);
        }

        // Sum should be preserved after many rounds
        let final_sum: f64 = [
            budget.local_reserve,
            budget.cell_share,
            budget.org_share,
            budget.federation_share,
            budget.commons_share,
        ]
        .iter()
        .sum();
        assert!(
            (final_sum - original_sum).abs() < 0.01,
            "Sum should be preserved: original={original_sum}, final={final_sum}"
        );

        // Ordering should reflect demand: Local > Cell > Org > Fed > Commons
        assert!(budget.local_reserve > budget.cell_share);
        assert!(budget.cell_share > budget.org_share);

        // All fractions should remain valid
        assert!(budget.validate().is_ok());
    }

    #[test]
    fn test_demand_adjustment_partial_data() {
        let mut budget = CapacityBudget::default();
        let original = budget.clone();

        // Only provide metrics for 2 out of 5 scopes
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Local, 0.90);
        utilization.insert(ScopeLevel::Commons, 0.10);

        budget.adjust_from_demand(&utilization, 0.05);

        // Budget should still be valid
        assert!(budget.validate().is_ok());

        // Scopes without data (Cell, Org, Federation) should be unchanged
        assert!(
            (budget.cell_share - original.cell_share).abs() < 1e-6,
            "Cell share should be unchanged: {} vs {}",
            budget.cell_share,
            original.cell_share
        );
        assert!(
            (budget.org_share - original.org_share).abs() < 1e-6,
            "Org share should be unchanged: {} vs {}",
            budget.org_share,
            original.org_share
        );
        assert!(
            (budget.federation_share - original.federation_share).abs() < 1e-6,
            "Federation share should be unchanged: {} vs {}",
            budget.federation_share,
            original.federation_share
        );

        // Local (high demand) should gain, Commons (low demand) should lose
        assert!(budget.local_reserve > original.local_reserve);
        assert!(budget.commons_share < original.commons_share);
    }

    #[test]
    fn test_scope_budget_exhaustion() {
        let policy = DefaultPlacementPolicy::default();
        let profile = ResourceProfile::minimal();

        // Node with 8 cores, default budget gives Cell 25% → 2.0 CPU for Cell
        // At 0.1 CPU/task (DEFAULT_CPU_PER_TASK) → 20 task slots for Cell scope
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
            queue_depth: 0,
            scope_queue_depths: HashMap::new(),
        };
        let task_hash = [0u8; 32];
        let empty_locality = LocalityContext::empty();
        let request = PlacementRequest {
            task_hash,
            resource_profile: profile.clone(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: None,
            cell_affinity: None,
            allowed_scopes: vec![],
        };

        // Cell scope with queue under budget → accepted
        let cell_ctx_low = ScopeContext {
            peer_scope: ScopeLevel::Cell,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        let mut under_budget_state = node_state.clone();
        under_budget_state
            .scope_queue_depths
            .insert(ScopeLevel::Cell, 10);
        let offer = policy.score_task(
            &request,
            "did:icn:alice",
            &under_budget_state,
            0.8,
            &empty_locality,
            &cell_ctx_low,
        );
        assert!(offer.is_some(), "Should accept when queue is under budget");

        // Cell scope with queue over budget → rejected
        let mut over_budget_state = node_state.clone();
        over_budget_state
            .scope_queue_depths
            .insert(ScopeLevel::Cell, 25);
        let offer = policy.score_task(
            &request,
            "did:icn:alice",
            &over_budget_state,
            0.8,
            &empty_locality,
            &cell_ctx_low,
        );
        assert!(
            offer.is_none(),
            "Should reject when scope queue exceeds budget"
        );
    }

    #[test]
    fn test_allowed_scopes_empty_means_any() {
        let policy = DefaultPlacementPolicy::default();
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
            queue_depth: 0,
            scope_queue_depths: HashMap::new(),
        };
        let empty_locality = LocalityContext::empty();

        // Empty allowed_scopes → any scope accepted
        let request = PlacementRequest {
            task_hash: [0u8; 32],
            resource_profile: ResourceProfile::minimal(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: None,
            cell_affinity: None,
            allowed_scopes: vec![],
        };

        for &scope in &ScopeLevel::ALL {
            let ctx = ScopeContext {
                peer_scope: scope,
                executor_cell: None,
                capacity_budget: CapacityBudget::default(),
            };
            assert!(
                policy
                    .score_task(
                        &request,
                        "did:icn:alice",
                        &node_state,
                        0.8,
                        &empty_locality,
                        &ctx
                    )
                    .is_some(),
                "Empty allowed_scopes should accept scope {scope:?}"
            );
        }
    }

    #[test]
    fn test_allowed_scopes_filters_executor() {
        let policy = DefaultPlacementPolicy::default();
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
            queue_depth: 0,
            scope_queue_depths: HashMap::new(),
        };
        let empty_locality = LocalityContext::empty();

        // Only Org allowed → Cell executor rejected
        let request = PlacementRequest {
            task_hash: [0u8; 32],
            resource_profile: ResourceProfile::minimal(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: None,
            cell_affinity: None,
            allowed_scopes: vec![ScopeLevel::Org],
        };

        let cell_ctx = ScopeContext {
            peer_scope: ScopeLevel::Cell,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        assert!(
            policy
                .score_task(
                    &request,
                    "did:icn:alice",
                    &node_state,
                    0.8,
                    &empty_locality,
                    &cell_ctx,
                )
                .is_none(),
            "Cell executor should be rejected when only Org is allowed"
        );
    }

    #[test]
    fn test_allowed_scopes_accepts_matching() {
        let policy = DefaultPlacementPolicy::default();
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
            queue_depth: 0,
            scope_queue_depths: HashMap::new(),
        };
        let empty_locality = LocalityContext::empty();

        // Cell and Federation allowed
        let request = PlacementRequest {
            task_hash: [0u8; 32],
            resource_profile: ResourceProfile::minimal(),
            locality_hints: vec![],
            max_cost: None,
            requested_at: 1000,
            max_scope: None,
            cell_affinity: None,
            allowed_scopes: vec![ScopeLevel::Cell, ScopeLevel::Federation],
        };

        let cell_ctx = ScopeContext {
            peer_scope: ScopeLevel::Cell,
            executor_cell: None,
            capacity_budget: CapacityBudget::default(),
        };
        assert!(
            policy
                .score_task(
                    &request,
                    "did:icn:alice",
                    &node_state,
                    0.8,
                    &empty_locality,
                    &cell_ctx,
                )
                .is_some(),
            "Cell executor should be accepted when Cell is in allowed list"
        );
    }

    #[test]
    fn test_demand_adjustment_increases_busy_scope() {
        let mut budget = CapacityBudget::default();
        // Simulate heavy Cell demand
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Cell, 0.8);
        utilization.insert(ScopeLevel::Org, 0.1);
        utilization.insert(ScopeLevel::Commons, 0.05);
        utilization.insert(ScopeLevel::Federation, 0.03);
        utilization.insert(ScopeLevel::Commons, 0.02);

        let cell_before = budget.fraction_for(ScopeLevel::Cell);
        budget.adjust_from_demand(&utilization, 0.05);
        let cell_after = budget.fraction_for(ScopeLevel::Cell);

        assert!(
            cell_after > cell_before,
            "Cell fraction should increase when Cell has high demand: before={cell_before}, after={cell_after}"
        );
    }

    #[test]
    fn test_demand_adjustment_preserves_sum() {
        let mut budget = CapacityBudget::default();
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Cell, 0.6);
        utilization.insert(ScopeLevel::Org, 0.3);
        utilization.insert(ScopeLevel::Commons, 0.1);

        budget.adjust_from_demand(&utilization, 0.05);

        let total: f64 = ScopeLevel::ALL
            .iter()
            .map(|&s| budget.fraction_for(s))
            .sum();
        assert!(
            (total - 1.0).abs() < 1e-6,
            "Budget fractions should sum to ~1.0 after adjustment, got {total}"
        );
    }

    #[test]
    fn test_demand_config_defaults() {
        let config = DemandAdjustmentConfig::default();
        assert_eq!(config.interval_secs, 60);
        assert!((config.learning_rate - 0.05).abs() < 1e-9);
        assert_eq!(config.min_samples, 5);
    }

    #[test]
    fn test_budget_passed_to_scope_context() {
        // Verify that a non-default budget is used by ScopeContext
        let mut budget = CapacityBudget::default();
        let mut utilization = HashMap::new();
        utilization.insert(ScopeLevel::Cell, 0.9);
        utilization.insert(ScopeLevel::Commons, 0.1);
        budget.adjust_from_demand(&utilization, 0.05);

        let ctx = ScopeContext {
            peer_scope: ScopeLevel::Cell,
            executor_cell: None,
            capacity_budget: budget.clone(),
        };

        // The budget in the context should match what we set
        assert!(
            (ctx.capacity_budget.fraction_for(ScopeLevel::Cell)
                - budget.fraction_for(ScopeLevel::Cell))
            .abs()
                < 1e-9,
            "ScopeContext should carry the provided budget"
        );
    }
}
