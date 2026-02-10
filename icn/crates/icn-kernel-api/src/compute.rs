//! Compute Primitive
//!
//! Provides WASM execution with metering, scheduling, and resource quotas.
//!
//! # Design
//!
//! Two execution modes:
//! - **Deterministic**: No I/O, no wall clock, same input → same output.
//!   Used for reducers, validators, governance tallying.
//! - **Service**: Capability-gated I/O, long-running processes.
//!   Used for indexers, bridges, WebSocket handlers.
//!
//! # Determinism Classification (E3)
//!
//! Workloads are classified by their ability to mutate canonical state:
//! - **Canonical**: Must be deterministic; outputs can mutate ledger, governance, trust.
//! - **Advisory**: Can be probabilistic; outputs are suggestions only, cannot mutate state.
//!
//! This is the "split the universe" rule: only Canonical outputs can become state.
//!
//! # ABIs
//!
//! - `icn_abi_v1`: Deterministic compute
//! - `icn_service_abi_v1`: Service compute
//!
//! ABIs are versioned and stable (like syscalls).
//!
//! # Non-Goals
//!
//! - Domain-specific libraries (apps bundle their own)
//! - Privileged execution for "official" apps
//! - Any interpretation of what code does

use crate::types::{Duration, JobId, LogId, LogicalTimestamp, ServiceId};
use serde::{Deserialize, Serialize};

// ============================================================================
// Determinism Classification (E3)
// ============================================================================

/// Classification for whether workload outputs can mutate canonical state.
///
/// This is the core "split the universe" rule: only Canonical workloads can
/// mutate ledger state, governance records, trust edges, or any other
/// state that affects future constraints.
///
/// # Class A: Canonical Workloads
///
/// Must be deterministic. Outputs become state. Examples:
/// - Governance tallying
/// - Budget computation
/// - Settlement/netting
/// - Eligibility evaluation
/// - Trust edge updates
/// - Credit limit calculation
///
/// # Class B: Advisory Workloads
///
/// Can be probabilistic. Outputs are suggestions only. Examples:
/// - Demand forecasting
/// - Routing optimization
/// - Recommendations
/// - Anomaly detection
/// - Scheduling suggestions
/// - Predictive maintenance
///
/// # Enforcement
///
/// The kernel enforces this classification at execution boundaries:
/// - Scheduler rejects placement of Advisory tasks with WriteLedger capability
/// - CCL interpreter rejects ledger mutations from Advisory contexts
/// - Execution receipts track which class produced the output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    /// Must be deterministic; outputs can mutate canonical state.
    #[default]
    Canonical,
    /// Can be probabilistic; outputs are suggestions only.
    Advisory,
}

impl DeterminismClass {
    /// Returns true if workloads of this class can mutate canonical state.
    #[inline]
    pub const fn can_mutate_state(&self) -> bool {
        matches!(self, DeterminismClass::Canonical)
    }

    /// Returns true if workloads must produce deterministic outputs.
    #[inline]
    pub const fn requires_determinism(&self) -> bool {
        matches!(self, DeterminismClass::Canonical)
    }

    /// Returns true if this is an advisory/suggestion-only workload.
    #[inline]
    pub const fn is_advisory(&self) -> bool {
        matches!(self, DeterminismClass::Advisory)
    }
}

/// Privacy classification for workload inputs and outputs.
///
/// Maps directly to audit visibility tiers. Controls who can observe
/// task inputs, outputs, and execution traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    /// Anyone can see inputs, outputs, and execution traces.
    #[default]
    Public,
    /// Only organization/cooperative members can observe.
    Member,
    /// Selective disclosure via ZK proofs; only authorized parties see data.
    NeedToKnow,
}

impl PrivacyClass {
    /// Returns true if data is visible to the general public.
    #[inline]
    pub const fn visible_to_public(&self) -> bool {
        matches!(self, PrivacyClass::Public)
    }

    /// Returns true if membership verification is required for access.
    #[inline]
    pub const fn requires_membership(&self) -> bool {
        matches!(self, PrivacyClass::Member | PrivacyClass::NeedToKnow)
    }

    /// Returns true if ZK proof verification is required for access.
    #[inline]
    pub const fn requires_zk_proof(&self) -> bool {
        matches!(self, PrivacyClass::NeedToKnow)
    }
}

/// WASM module ready for execution.
#[derive(Clone, Debug)]
pub struct WasmModule {
    /// Module name
    pub name: String,
    /// Compiled WASM bytecode
    pub bytecode: Vec<u8>,
    /// ABI version this module uses
    pub abi_version: AbiVersion,
}

/// Supported ABI versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiVersion {
    /// Deterministic compute ABI (v1)
    DeterministicV1,
    /// Service compute ABI (v1)
    ServiceV1,
}

/// Result of deterministic execution.
#[derive(Clone, Debug)]
pub struct ExecutionResult {
    /// Output bytes
    pub output: Vec<u8>,
    /// Fuel consumed
    pub fuel_consumed: u64,
    /// Execution time (for metrics, not determinism)
    pub execution_time_us: u64,
}

/// Service configuration.
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    /// Service name
    pub name: String,
    /// Environment variables
    pub env: std::collections::HashMap<String, String>,
    /// Capabilities granted to this service
    pub capabilities: Vec<String>,
}

/// Resource quota for execution.
#[derive(Clone, Debug)]
pub struct ResourceQuota {
    /// Maximum fuel (compute units) per execution
    pub max_fuel: u64,
    /// Maximum memory in bytes
    pub max_memory: u64,
    /// Maximum execution time
    pub max_duration: Duration,
    /// Maximum storage writes per execution
    pub max_storage_writes: u32,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_fuel: 1_000_000,
            max_memory: 64 * 1024 * 1024, // 64 MB
            max_duration: Duration::from_secs(30),
            max_storage_writes: 100,
        }
    }
}

/// Job definition for scheduled execution.
#[derive(Clone, Debug)]
pub struct Job {
    /// Unique job identifier
    pub id: JobId,
    /// WASM module to execute
    pub module: WasmModule,
    /// Input data for the job
    pub input: Vec<u8>,
    /// Resource quota for this job
    pub quota: ResourceQuota,
    /// Job metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Trigger condition for job execution.
#[derive(Clone, Debug)]
pub enum Trigger {
    /// Execute immediately
    Immediate,
    /// Execute when event appears in log
    OnEvent {
        /// Log to watch
        log_id: LogId,
        /// Optional filter for events
        filter: Option<Vec<u8>>,
    },
    /// Execute at a specific logical time
    AtTime {
        /// Target timestamp
        time: LogicalTimestamp,
    },
    /// Execute repeatedly at an interval
    Every {
        /// Interval between executions
        interval: Duration,
    },
    /// Execute when another job completes
    AfterJob {
        /// Job to wait for
        job_id: JobId,
    },
}

/// Job status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    /// Job is waiting to be scheduled
    Pending,
    /// Job is currently executing
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed with error
    Failed {
        /// Error message
        error: String,
    },
    /// Job was cancelled
    Cancelled,
}

/// Service status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is starting up
    Starting,
    /// Service is running
    Running,
    /// Service is stopping
    Stopping,
    /// Service has stopped
    Stopped,
    /// Service crashed
    Crashed {
        /// Error message
        error: String,
    },
}

/// Compute engine for WASM execution.
pub trait ComputeEngine: Send + Sync {
    /// Execute a deterministic computation.
    ///
    /// The module must use `icn_abi_v1` (deterministic).
    /// Same input always produces same output.
    fn execute_deterministic(
        &self,
        module: &WasmModule,
        input: &[u8],
        fuel: u64,
    ) -> Result<ExecutionResult, ComputeError>;

    /// Start a long-running service.
    ///
    /// The module must use `icn_service_abi_v1`.
    fn start_service(
        &self,
        module: &WasmModule,
        config: ServiceConfig,
        quota: ResourceQuota,
    ) -> Result<ServiceId, ComputeError>;

    /// Stop a running service.
    fn stop_service(&self, id: &ServiceId) -> Result<(), ComputeError>;

    /// Get service status.
    fn service_status(&self, id: &ServiceId) -> Result<ServiceStatus, ComputeError>;

    /// List running services.
    fn list_services(&self) -> Result<Vec<ServiceId>, ComputeError>;

    /// Schedule a job for later execution.
    fn schedule(&self, job: Job, trigger: Trigger) -> Result<JobId, ComputeError>;

    /// Cancel a scheduled job.
    fn cancel(&self, job_id: &JobId) -> Result<(), ComputeError>;

    /// Get job status.
    fn job_status(&self, job_id: &JobId) -> Result<JobStatus, ComputeError>;

    /// List scheduled jobs.
    fn list_jobs(&self) -> Result<Vec<JobId>, ComputeError>;
}

/// Errors from compute operations.
#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    /// Module not found
    #[error("Module not found: {0}")]
    ModuleNotFound(String),

    /// Invalid WASM module
    #[error("Invalid WASM module: {0}")]
    InvalidModule(String),

    /// ABI version mismatch
    #[error("ABI version mismatch: expected {expected:?}, got {got:?}")]
    AbiMismatch {
        expected: AbiVersion,
        got: AbiVersion,
    },

    /// Fuel exhausted during execution
    #[error("Fuel exhausted")]
    FuelExhausted,

    /// Memory limit exceeded
    #[error("Memory limit exceeded")]
    MemoryExceeded,

    /// Execution timeout
    #[error("Execution timeout")]
    Timeout,

    /// Service not found
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    /// Service already running
    #[error("Service already running: {0}")]
    ServiceAlreadyRunning(String),

    /// Job not found
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Execution error (trap, panic, etc.)
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// Resource quota exceeded
    #[error("Resource quota exceeded: {0}")]
    QuotaExceeded(String),

    /// Advisory task attempted state mutation (E3 enforcement)
    #[error("Advisory workload cannot mutate state: {operation}")]
    AdvisoryMutationRejected {
        /// The mutation operation that was rejected
        operation: String,
    },

    /// Internal error
    #[error("Compute error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_version() {
        assert_eq!(AbiVersion::DeterministicV1, AbiVersion::DeterministicV1);
        assert_ne!(AbiVersion::DeterministicV1, AbiVersion::ServiceV1);
    }

    #[test]
    fn test_resource_quota_default() {
        let quota = ResourceQuota::default();
        assert_eq!(quota.max_fuel, 1_000_000);
        assert_eq!(quota.max_memory, 64 * 1024 * 1024);
        assert_eq!(quota.max_duration, Duration::from_secs(30));
        assert_eq!(quota.max_storage_writes, 100);
    }

    #[test]
    fn test_job_status() {
        assert_eq!(JobStatus::Pending, JobStatus::Pending);
        assert_ne!(JobStatus::Pending, JobStatus::Running);

        let failed = JobStatus::Failed {
            error: "test".to_string(),
        };
        match failed {
            JobStatus::Failed { error } => assert_eq!(error, "test"),
            _ => panic!("Expected Failed"),
        }
    }

    #[test]
    fn test_trigger_variants() {
        let _immediate = Trigger::Immediate;
        let _at_time = Trigger::AtTime { time: 12345 };
        let _every = Trigger::Every {
            interval: Duration::from_secs(60),
        };
    }

    #[test]
    fn test_determinism_class_canonical_default() {
        let class: DeterminismClass = Default::default();
        assert_eq!(class, DeterminismClass::Canonical);
        assert!(class.can_mutate_state());
        assert!(class.requires_determinism());
        assert!(!class.is_advisory());
    }

    #[test]
    fn test_determinism_class_advisory() {
        let class = DeterminismClass::Advisory;
        assert!(!class.can_mutate_state());
        assert!(!class.requires_determinism());
        assert!(class.is_advisory());
    }

    #[test]
    fn test_determinism_class_serde() {
        let canonical = DeterminismClass::Canonical;
        let json = serde_json::to_string(&canonical).unwrap();
        assert_eq!(json, r#""canonical""#);

        let advisory = DeterminismClass::Advisory;
        let json = serde_json::to_string(&advisory).unwrap();
        assert_eq!(json, r#""advisory""#);

        let parsed: DeterminismClass = serde_json::from_str(r#""advisory""#).unwrap();
        assert_eq!(parsed, DeterminismClass::Advisory);
    }

    #[test]
    fn test_privacy_class_defaults() {
        let class: PrivacyClass = Default::default();
        assert_eq!(class, PrivacyClass::Public);
        assert!(class.visible_to_public());
        assert!(!class.requires_membership());
        assert!(!class.requires_zk_proof());
    }

    #[test]
    fn test_privacy_class_member() {
        let class = PrivacyClass::Member;
        assert!(!class.visible_to_public());
        assert!(class.requires_membership());
        assert!(!class.requires_zk_proof());
    }

    #[test]
    fn test_privacy_class_need_to_know() {
        let class = PrivacyClass::NeedToKnow;
        assert!(!class.visible_to_public());
        assert!(class.requires_membership());
        assert!(class.requires_zk_proof());
    }

    #[test]
    fn test_advisory_mutation_rejected_error() {
        let err = ComputeError::AdvisoryMutationRejected {
            operation: "WriteLedger".to_string(),
        };
        assert!(err
            .to_string()
            .contains("Advisory workload cannot mutate state"));
        assert!(err.to_string().contains("WriteLedger"));
    }
}
