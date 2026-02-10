//! Compute service for task submission and management

use crate::error::ApiError;
use crate::ApiContext;
use icn_compute::{
    ComputeHandle, ComputeTask, DeterminismClass, ExecutorCapability, FuelLimit, PrivacyClass,
    ResourceProfile, TaskCode, TaskPriority, TaskStatus,
};

// ============================================================================
// Validation Constants
// ============================================================================

/// Minimum task ID length
pub const MIN_TASK_ID_LEN: usize = 1;
/// Maximum task ID length
pub const MAX_TASK_ID_LEN: usize = 256;
/// Minimum fuel limit for compute tasks
pub const MIN_FUEL_LIMIT: u64 = 100;
/// Maximum fuel limit for compute tasks
pub const MAX_FUEL_LIMIT: u64 = 10_000_000;
/// Maximum payment rate per unit of fuel
pub const MAX_PAYMENT_RATE: u64 = 1_000_000;
/// Minimum CPU cores for resource profile (practical minimum)
pub const MIN_CPU_CORES: f64 = 0.1;
/// Maximum CPU cores for resource profile
pub const MAX_CPU_CORES: f64 = 256.0;
/// Minimum memory in MB for resource profile
pub const MIN_MEMORY_MB: u64 = 1;
/// Maximum memory in MB for resource profile
pub const MAX_MEMORY_MB: u64 = 1_000_000;
/// Maximum storage in MB for resource profile
pub const MAX_STORAGE_MB: u64 = 10_000_000;
/// Minimum network bandwidth in Mbps
pub const MIN_NETWORK_MBPS: f64 = 0.1;
/// Maximum network bandwidth in Mbps
pub const MAX_NETWORK_MBPS: f64 = 100_000.0;

/// Task ID type
pub type TaskId = [u8; 32];

/// Compute service provides shared compute task operations
pub struct ComputeService {
    compute_handle: ComputeHandle,
}

impl ComputeService {
    /// Create a new compute service
    pub fn new(compute_handle: ComputeHandle) -> Self {
        Self { compute_handle }
    }

    /// Submit a compute task
    pub async fn submit_task(
        &self,
        ctx: &ApiContext,
        params: SubmitTaskParams,
    ) -> Result<TaskId, ApiError> {
        // Validate parameters
        params.validate()?;

        // Build TaskCode based on code type
        let (task_code, required_capabilities) = match params.code_type {
            CodeTypeParam::Ccl => {
                let code = params.code.ok_or_else(|| {
                    ApiError::InvalidParameter("Missing 'code' for CCL task".into())
                })?;
                (TaskCode::Ccl(code), vec![ExecutorCapability::Ccl])
            }
            CodeTypeParam::Wasm => {
                let wasm_b64 = params.wasm_bytes.ok_or_else(|| {
                    ApiError::InvalidParameter("Missing 'wasm_bytes' for WASM task".into())
                })?;
                let wasm_bytes =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &wasm_b64)
                        .map_err(|e| ApiError::InvalidParameter(format!("Invalid base64: {e}")))?;
                (
                    TaskCode::WasmInline(wasm_bytes),
                    vec![ExecutorCapability::Wasm],
                )
            }
        };

        // Convert priority
        let priority = match params.priority {
            TaskPriorityParam::Low => TaskPriority::Low,
            TaskPriorityParam::Normal => TaskPriority::Normal,
            TaskPriorityParam::High => TaskPriority::High,
            TaskPriorityParam::Critical => TaskPriority::Critical,
        };

        // Build resource profile if provided
        let resource_profile = params.resource_profile.map(|rp| ResourceProfile {
            cpu_cores: rp.cpu_cores,
            memory_mb: rp.memory_mb,
            storage_mb: rp.storage_mb,
            network_mbps: rp.network_mbps,
            gpu_spec: None,
            duration_estimate: rp
                .duration_estimate_secs
                .map(std::time::Duration::from_secs),
        });

        // Convert inputs to bytes
        let inputs = if params.inputs.is_null() {
            vec![]
        } else {
            serde_json::to_vec(&params.inputs)
                .map_err(|e| ApiError::Internal(format!("Failed to serialize inputs: {e}")))?
        };

        // Get coop_id: prefer request, fallback to context
        let coop_id = params.coop_id.or_else(|| ctx.coop_id.clone());

        // Convert relative deadline to absolute timestamp
        let now = icn_time::current_timestamp_millis();
        let deadline = params.deadline_ms.map(|relative_ms| now + relative_ms);

        // Build the compute task
        let task = ComputeTask {
            id: params.task_id,
            submitter: ctx.caller_did.clone(),
            coop_id,
            code: task_code,
            inputs,
            fuel_limit: FuelLimit(params.fuel_limit),
            required_capabilities,
            priority,
            created_at: now,
            deadline,
            payment_rate: params.payment_rate,
            payment_currency: params.payment_currency,
            resource_profile,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            // E1: Workload manifest fields
            inputs_hash: None, // TODO: Compute from inputs when provided
            policy_hash: None, // TODO: Add to API params
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
        };

        // Submit the task
        self.compute_handle
            .submit(task)
            .await
            .map_err(|e| ApiError::ComputeError(e.to_string()))
    }

    /// Get task status
    pub async fn get_status(
        &self,
        _ctx: &ApiContext,
        task_id: &TaskId,
    ) -> Result<TaskStatusResponse, ApiError> {
        match self.compute_handle.status(*task_id).await {
            Ok(Some(status)) => {
                let response = convert_task_status(task_id, status);
                Ok(response)
            }
            Ok(None) => Err(ApiError::TaskNotFound(hex::encode(task_id))),
            Err(e) => Err(ApiError::ComputeError(e.to_string())),
        }
    }

    /// Cancel a task
    pub async fn cancel_task(
        &self,
        ctx: &ApiContext,
        task_id: &TaskId,
        reason: Option<String>,
    ) -> Result<(), ApiError> {
        let reason = reason.unwrap_or_else(|| "Cancelled by submitter".to_string());

        self.compute_handle
            .cancel_task(task_id, &ctx.caller_did, reason)
            .await
            .map_err(|e| ApiError::ComputeError(e.to_string()))
    }
}

/// Parameters for submitting a compute task
#[derive(Debug, Clone)]
pub struct SubmitTaskParams {
    pub task_id: String,
    pub code: Option<String>,
    pub wasm_bytes: Option<String>,
    pub code_type: CodeTypeParam,
    pub inputs: serde_json::Value,
    pub fuel_limit: u64,
    pub priority: TaskPriorityParam,
    pub deadline_ms: Option<u64>,
    pub payment_rate: Option<u64>,
    pub payment_currency: Option<String>,
    pub coop_id: Option<String>,
    pub resource_profile: Option<ResourceProfileParam>,
}

impl SubmitTaskParams {
    /// Validate task parameters
    pub fn validate(&self) -> Result<(), ApiError> {
        // Validate task_id length
        if self.task_id.len() < MIN_TASK_ID_LEN {
            return Err(ApiError::ValidationError(format!(
                "Task ID cannot be empty (min {} char)",
                MIN_TASK_ID_LEN
            )));
        }
        if self.task_id.len() > MAX_TASK_ID_LEN {
            return Err(ApiError::ValidationError(format!(
                "Task ID too long (max {} chars)",
                MAX_TASK_ID_LEN
            )));
        }

        // Validate code is not empty (if provided)
        if let Some(ref code) = self.code {
            if code.is_empty() {
                return Err(ApiError::ValidationError("Code cannot be empty".into()));
            }
        }

        // Validate wasm_bytes is not empty (if provided)
        if let Some(ref wasm) = self.wasm_bytes {
            if wasm.is_empty() {
                return Err(ApiError::ValidationError(
                    "WASM bytes cannot be empty".into(),
                ));
            }
        }

        // Validate fuel limit
        if self.fuel_limit < MIN_FUEL_LIMIT {
            return Err(ApiError::ValidationError(format!(
                "Fuel limit too low (min {})",
                MIN_FUEL_LIMIT
            )));
        }
        if self.fuel_limit > MAX_FUEL_LIMIT {
            return Err(ApiError::ValidationError(format!(
                "Fuel limit too high (max {})",
                MAX_FUEL_LIMIT
            )));
        }

        // Validate payment rate if provided
        if let Some(rate) = self.payment_rate {
            if rate > MAX_PAYMENT_RATE {
                return Err(ApiError::ValidationError(format!(
                    "Payment rate too high (max {})",
                    MAX_PAYMENT_RATE
                )));
            }
        }

        // Validate resource profile if provided
        if let Some(ref profile) = self.resource_profile {
            profile.validate()?;
        }

        Ok(())
    }
}

/// Code type parameter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeTypeParam {
    Ccl,
    Wasm,
}

/// Task priority parameter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPriorityParam {
    Low,
    Normal,
    High,
    Critical,
}

impl TaskPriorityParam {
    /// Parse from string (case-insensitive)
    #[allow(clippy::should_implement_trait)] // Returns Self, not Result, so FromStr doesn't fit
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => TaskPriorityParam::Low,
            "high" => TaskPriorityParam::High,
            "critical" => TaskPriorityParam::Critical,
            _ => TaskPriorityParam::Normal, // Default
        }
    }
}

/// Resource profile parameter for specifying task resource requirements.
///
/// All fields are optional. An empty profile (all fields `None`) is valid and
/// indicates that the daemon should use its default resource allocation.
/// This is intentional to support simple tasks that don't need specific
/// resource constraints.
#[derive(Debug, Clone)]
pub struct ResourceProfileParam {
    pub cpu_cores: Option<f64>,
    pub memory_mb: Option<u64>,
    pub storage_mb: Option<u64>,
    pub network_mbps: Option<f64>,
    pub duration_estimate_secs: Option<u64>,
}

impl ResourceProfileParam {
    /// Validate resource profile
    pub fn validate(&self) -> Result<(), ApiError> {
        if let Some(cpu) = self.cpu_cores {
            if !(MIN_CPU_CORES..=MAX_CPU_CORES).contains(&cpu) {
                return Err(ApiError::ValidationError(format!(
                    "CPU cores must be between {} and {}",
                    MIN_CPU_CORES, MAX_CPU_CORES
                )));
            }
        }

        if let Some(mem) = self.memory_mb {
            if !(MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&mem) {
                return Err(ApiError::ValidationError(format!(
                    "Memory must be between {} and {} MB",
                    MIN_MEMORY_MB, MAX_MEMORY_MB
                )));
            }
        }

        if let Some(storage) = self.storage_mb {
            if storage > MAX_STORAGE_MB {
                return Err(ApiError::ValidationError(format!(
                    "Storage must be <= {} MB",
                    MAX_STORAGE_MB
                )));
            }
        }

        if let Some(net) = self.network_mbps {
            if !(MIN_NETWORK_MBPS..=MAX_NETWORK_MBPS).contains(&net) {
                return Err(ApiError::ValidationError(format!(
                    "Network bandwidth must be between {} and {} Mbps",
                    MIN_NETWORK_MBPS, MAX_NETWORK_MBPS
                )));
            }
        }

        Ok(())
    }
}

/// Task status response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskStatusResponse {
    pub task_hash: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResultResponse>,
}

/// Task result response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskResultResponse {
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub fuel_used: u64,
    pub duration_ms: u64,
}

/// Convert icn-compute TaskStatus to API response
fn convert_task_status(task_id: &TaskId, status: TaskStatus) -> TaskStatusResponse {
    let task_hash = hex::encode(task_id);

    match status {
        TaskStatus::Pending => TaskStatusResponse {
            task_hash,
            status: "pending".to_string(),
            executor: None,
            result: None,
        },
        TaskStatus::Claimed { executor, .. } => TaskStatusResponse {
            task_hash,
            status: "claimed".to_string(),
            executor: Some(executor),
            result: None,
        },
        TaskStatus::Completed { result } => {
            let (outcome, output, error) = match &result.outcome {
                icn_compute::ExecutionOutcome::Success(data) => {
                    let output: Option<serde_json::Value> = serde_json::from_slice(data).ok();
                    ("success".to_string(), output, None)
                }
                icn_compute::ExecutionOutcome::Failed(e) => {
                    ("failed".to_string(), None, Some(e.clone()))
                }
                icn_compute::ExecutionOutcome::OutOfFuel => ("out_of_fuel".to_string(), None, None),
                icn_compute::ExecutionOutcome::Timeout => ("timeout".to_string(), None, None),
            };

            TaskStatusResponse {
                task_hash,
                status: "completed".to_string(),
                executor: Some(result.executor.clone()),
                result: Some(TaskResultResponse {
                    outcome,
                    output,
                    error,
                    fuel_used: result.fuel_used,
                    duration_ms: result.duration_ms,
                }),
            }
        }
        TaskStatus::Failed { reason } => TaskStatusResponse {
            task_hash,
            status: "failed".to_string(),
            executor: None,
            result: Some(TaskResultResponse {
                outcome: "failed".to_string(),
                output: None,
                error: Some(reason),
                fuel_used: 0,
                duration_ms: 0,
            }),
        },
        TaskStatus::Cancelled { reason, .. } => TaskStatusResponse {
            task_hash,
            status: "cancelled".to_string(),
            executor: None,
            result: Some(TaskResultResponse {
                outcome: "cancelled".to_string(),
                output: None,
                error: Some(reason),
                fuel_used: 0,
                duration_ms: 0,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_priority_from_str() {
        assert_eq!(TaskPriorityParam::from_str("low"), TaskPriorityParam::Low);
        assert_eq!(TaskPriorityParam::from_str("LOW"), TaskPriorityParam::Low);
        assert_eq!(TaskPriorityParam::from_str("high"), TaskPriorityParam::High);
        assert_eq!(
            TaskPriorityParam::from_str("critical"),
            TaskPriorityParam::Critical
        );
        assert_eq!(
            TaskPriorityParam::from_str("invalid"),
            TaskPriorityParam::Normal
        );
    }

    #[test]
    fn test_validate_task_id() {
        let mut params = SubmitTaskParams {
            task_id: "valid-task-id".to_string(),
            code: Some("{}".to_string()),
            wasm_bytes: None,
            code_type: CodeTypeParam::Ccl,
            inputs: serde_json::Value::Null,
            fuel_limit: 10_000,
            priority: TaskPriorityParam::Normal,
            deadline_ms: None,
            payment_rate: None,
            payment_currency: None,
            coop_id: None,
            resource_profile: None,
        };

        // Valid task ID should pass
        assert!(params.validate().is_ok());

        // Empty task ID should fail
        params.task_id = "".to_string();
        assert!(params.validate().is_err());

        // Too long task ID should fail
        params.task_id = "x".repeat(300);
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_validate_fuel_limit() {
        let mut params = SubmitTaskParams {
            task_id: "task-1".to_string(),
            code: Some("{}".to_string()),
            wasm_bytes: None,
            code_type: CodeTypeParam::Ccl,
            inputs: serde_json::Value::Null,
            fuel_limit: 10_000,
            priority: TaskPriorityParam::Normal,
            deadline_ms: None,
            payment_rate: None,
            payment_currency: None,
            coop_id: None,
            resource_profile: None,
        };

        // Valid fuel limit
        assert!(params.validate().is_ok());

        // Too low
        params.fuel_limit = 50;
        assert!(params.validate().is_err());

        // Too high
        params.fuel_limit = 20_000_000;
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_validate_payment_rate() {
        let mut params = SubmitTaskParams {
            task_id: "task-1".to_string(),
            code: Some("{}".to_string()),
            wasm_bytes: None,
            code_type: CodeTypeParam::Ccl,
            inputs: serde_json::Value::Null,
            fuel_limit: 10_000,
            priority: TaskPriorityParam::Normal,
            deadline_ms: None,
            payment_rate: Some(1000),
            payment_currency: None,
            coop_id: None,
            resource_profile: None,
        };

        // Valid payment rate
        assert!(params.validate().is_ok());

        // Too high
        params.payment_rate = Some(2_000_000);
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_validate_resource_profile() {
        let profile = ResourceProfileParam {
            cpu_cores: Some(2.0),
            memory_mb: Some(1024),
            storage_mb: Some(5000),
            network_mbps: Some(100.0),
            duration_estimate_secs: Some(60),
        };

        // Valid profile
        assert!(profile.validate().is_ok());

        // Invalid CPU
        let mut profile = profile.clone();
        profile.cpu_cores = Some(0.0);
        assert!(profile.validate().is_err());

        profile.cpu_cores = Some(300.0);
        assert!(profile.validate().is_err());

        // Invalid memory
        let mut profile = ResourceProfileParam {
            cpu_cores: Some(2.0),
            memory_mb: Some(0),
            storage_mb: Some(5000),
            network_mbps: Some(100.0),
            duration_estimate_secs: Some(60),
        };
        assert!(profile.validate().is_err());

        profile.memory_mb = Some(2_000_000);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn test_empty_resource_profile_is_valid() {
        // An empty resource profile (all None) is valid - daemon uses defaults
        let profile = ResourceProfileParam {
            cpu_cores: None,
            memory_mb: None,
            storage_mb: None,
            network_mbps: None,
            duration_estimate_secs: None,
        };
        assert!(
            profile.validate().is_ok(),
            "Empty resource profile should be valid (uses daemon defaults)"
        );
    }

    #[test]
    fn test_fuel_limit_boundary_values() {
        let mut params = SubmitTaskParams {
            task_id: "task-1".to_string(),
            code: Some("{}".to_string()),
            wasm_bytes: None,
            code_type: CodeTypeParam::Ccl,
            inputs: serde_json::Value::Null,
            fuel_limit: MIN_FUEL_LIMIT,
            priority: TaskPriorityParam::Normal,
            deadline_ms: None,
            payment_rate: None,
            payment_currency: None,
            coop_id: None,
            resource_profile: None,
        };

        // Exactly MIN_FUEL_LIMIT should pass
        assert!(
            params.validate().is_ok(),
            "MIN_FUEL_LIMIT ({}) should be valid",
            MIN_FUEL_LIMIT
        );

        // Exactly MAX_FUEL_LIMIT should pass
        params.fuel_limit = MAX_FUEL_LIMIT;
        assert!(
            params.validate().is_ok(),
            "MAX_FUEL_LIMIT ({}) should be valid",
            MAX_FUEL_LIMIT
        );

        // One below MIN should fail
        params.fuel_limit = MIN_FUEL_LIMIT - 1;
        assert!(
            params.validate().is_err(),
            "MIN_FUEL_LIMIT - 1 should be invalid"
        );

        // One above MAX should fail
        params.fuel_limit = MAX_FUEL_LIMIT + 1;
        assert!(
            params.validate().is_err(),
            "MAX_FUEL_LIMIT + 1 should be invalid"
        );
    }

    #[test]
    fn test_unicode_task_id() {
        // Unicode characters in task IDs are currently allowed
        // (length validation uses String::len() which counts bytes)
        let params = SubmitTaskParams {
            task_id: "task-日本語-🚀".to_string(),
            code: Some("{}".to_string()),
            wasm_bytes: None,
            code_type: CodeTypeParam::Ccl,
            inputs: serde_json::Value::Null,
            fuel_limit: 10_000,
            priority: TaskPriorityParam::Normal,
            deadline_ms: None,
            payment_rate: None,
            payment_currency: None,
            coop_id: None,
            resource_profile: None,
        };

        // Currently passes - document this behavior
        // Phase 3 security hardening may add stricter format validation
        assert!(
            params.validate().is_ok(),
            "Unicode task IDs are currently allowed"
        );
    }
}
