//! Compute Manager for Gateway API
//!
//! Provides distributed compute operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory task tracking (for standalone gateway)
//! 2. ComputeService (when integrated with daemon)

use crate::api::compute::ResourceProfileRequest;
use anyhow::Result;
use icn_compute::TaskCode;
use std::sync::Arc;

/// Hash type for task identification
pub type TaskHash = [u8; 32];

/// Compute manager for gateway API
pub struct ComputeManager {
    /// Compute service (shared layer)
    compute_service: Option<Arc<icn_api::ComputeService>>,
    /// WASM registry for module management
    wasm_registry: Option<Arc<icn_compute::WasmRegistry>>,
}

impl ComputeManager {
    /// Create a new compute manager (standalone mode)
    pub fn new() -> Self {
        ComputeManager {
            compute_service: None,
            wasm_registry: None,
        }
    }

    /// Create a compute manager with daemon connection
    pub fn with_service(service: Arc<icn_api::ComputeService>) -> Self {
        ComputeManager {
            compute_service: Some(service),
            wasm_registry: None,
        }
    }

    /// Create a compute manager with daemon connection and WASM registry
    pub fn with_service_and_registry(
        service: Arc<icn_api::ComputeService>,
        registry: Arc<icn_compute::WasmRegistry>,
    ) -> Self {
        ComputeManager {
            compute_service: Some(service),
            wasm_registry: Some(registry),
        }
    }

    /// Create a compute manager with only a WASM registry (standalone mode)
    pub fn with_registry(registry: Arc<icn_compute::WasmRegistry>) -> Self {
        ComputeManager {
            compute_service: None,
            wasm_registry: Some(registry),
        }
    }

    /// Set compute service (for late binding)
    pub fn set_service(&mut self, service: Arc<icn_api::ComputeService>) {
        self.compute_service = Some(service);
    }

    /// Set WASM registry for module management
    pub fn set_wasm_registry(&mut self, registry: Arc<icn_compute::WasmRegistry>) {
        self.wasm_registry = Some(registry);
    }

    /// Get a reference to the WASM registry (if configured)
    pub fn wasm_registry(&self) -> Option<&Arc<icn_compute::WasmRegistry>> {
        self.wasm_registry.as_ref()
    }

    /// Submit a compute task (CCL code)
    pub async fn submit_task(
        &self,
        task_id: String,
        submitter: String,
        coop_id: Option<String>,
        code: String,
        inputs: Vec<u8>,
        fuel_limit: u64,
        priority: &str,
        deadline_ms: Option<u64>,
        payment_rate: Option<u64>,
        payment_currency: Option<String>,
    ) -> Result<TaskHash> {
        self.submit_task_with_code(
            task_id,
            submitter,
            coop_id,
            TaskCode::Ccl(code),
            inputs,
            fuel_limit,
            priority,
            deadline_ms,
            payment_rate,
            payment_currency,
            None, // No resource profile for legacy API
        )
        .await
    }

    /// Submit a compute task with explicit TaskCode (CCL or WASM)
    pub async fn submit_task_with_code(
        &self,
        task_id: String,
        submitter: String,
        coop_id: Option<String>,
        code: TaskCode,
        inputs: Vec<u8>,
        fuel_limit: u64,
        priority: &str,
        deadline_ms: Option<u64>,
        payment_rate: Option<u64>,
        payment_currency: Option<String>,
        resource_profile: Option<ResourceProfileRequest>,
    ) -> Result<TaskHash> {
        let compute_service = self
            .compute_service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Compute not available - daemon not connected"))?;

        // Build API context
        let api_ctx = icn_api::ApiContext {
            caller_did: submitter.clone(),
            coop_id: coop_id.clone(),
        };

        // Convert TaskCode to API params.
        // WasmRef uses the distinct `wasm_hash` field so ComputeService can reconstruct
        // TaskCode::WasmRef rather than trying to base64-decode a hex string as bytes.
        let (code_str, wasm_bytes, wasm_hash, code_type) = match code {
            TaskCode::Ccl(c) => (Some(c), None, None, icn_api::compute::CodeTypeParam::Ccl),
            TaskCode::WasmInline(b) => {
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &b);
                (
                    None,
                    Some(encoded),
                    None,
                    icn_api::compute::CodeTypeParam::Wasm,
                )
            }
            TaskCode::CclRef { .. } => {
                anyhow::bail!("CclRef not supported via gateway API");
            }
            TaskCode::WasmRef(hash) => {
                let hash_hex = hex::encode(hash);
                (
                    None,
                    None,
                    Some(hash_hex),
                    icn_api::compute::CodeTypeParam::Wasm,
                )
            }
        };

        // Convert inputs to JSON
        let inputs_json = if inputs.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&inputs).unwrap_or(serde_json::Value::Null)
        };

        let params = icn_api::SubmitTaskParams {
            task_id,
            code: code_str,
            wasm_bytes,
            wasm_hash,
            code_type,
            inputs: inputs_json,
            fuel_limit,
            priority: icn_api::compute::TaskPriorityParam::from_str(priority),
            deadline_ms,
            payment_rate,
            payment_currency,
            coop_id: None, // Already in api_ctx
            resource_profile: resource_profile.map(|rp| icn_api::compute::ResourceProfileParam {
                cpu_cores: rp.cpu_cores,
                memory_mb: rp.memory_mb,
                storage_mb: rp.storage_mb,
                network_mbps: rp.network_mbps,
                duration_estimate_secs: rp.duration_estimate_secs,
            }),
        };

        compute_service
            .submit_task(&api_ctx, params)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to submit task: {e}"))
    }

    /// Get task status
    pub async fn get_status(&self, task_hash: TaskHash) -> Result<Option<ComputeTaskStatus>> {
        let compute_service = self
            .compute_service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Compute not available - daemon not connected"))?;

        // We need an API context for the service call - use anonymous for get_status
        let api_ctx = icn_api::ApiContext {
            caller_did: "did:icn:gateway-anonymous".to_string(),
            coop_id: None,
        };

        match compute_service.get_status(&api_ctx, &task_hash).await {
            Ok(status) => {
                // Convert API response to gateway response
                Ok(Some(ComputeTaskStatus {
                    task_hash: status.task_hash,
                    status: status.status,
                    executor: status.executor,
                    result: status.result.map(|r| ComputeResult {
                        outcome: r.outcome,
                        output: r.output,
                        error: r.error,
                        fuel_used: r.fuel_used,
                        duration_ms: r.duration_ms,
                    }),
                }))
            }
            Err(icn_api::ApiError::TaskNotFound(_)) => Ok(None),
            Err(e) => anyhow::bail!("Failed to get status: {e}"),
        }
    }

    /// Cancel a task
    pub async fn cancel_task(
        &self,
        task_hash: TaskHash,
        requester: String,
        reason: String,
    ) -> Result<()> {
        let compute_service = self
            .compute_service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Compute not available - daemon not connected"))?;

        let api_ctx = icn_api::ApiContext {
            caller_did: requester,
            coop_id: None,
        };

        compute_service
            .cancel_task(&api_ctx, &task_hash, Some(reason))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to cancel task: {e}"))
    }
}

impl Default for ComputeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Task status response
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ComputeTaskStatus {
    pub task_hash: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ComputeResult>,
}

/// Task result
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ComputeResult {
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub fuel_used: u64,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_manager_new() {
        let mgr = ComputeManager::new();
        assert!(mgr.compute_service.is_none());
    }

    #[tokio::test]
    async fn test_submit_without_daemon() {
        let mgr = ComputeManager::new();
        let result = mgr
            .submit_task(
                "task-1".to_string(),
                "did:icn:alice".to_string(),
                Some("test-coop".to_string()),
                r#"{"name": "Test"}"#.to_string(),
                vec![],
                10_000,
                "normal",
                None,
                None,
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    // Note: validation tests now happen in ComputeService (icn-api)
    // so we don't need to duplicate them here

    #[tokio::test]
    async fn test_get_status_without_daemon() {
        let mgr = ComputeManager::new();
        let hash = [0u8; 32];
        let result = mgr.get_status(hash).await;

        // Without daemon, should error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    #[tokio::test]
    async fn test_cancel_task_without_daemon() {
        let mgr = ComputeManager::new();
        let hash = [0u8; 32];
        let result = mgr
            .cancel_task(hash, "did:icn:alice".to_string(), "test".to_string())
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }
}
