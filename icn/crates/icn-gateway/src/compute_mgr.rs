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
    /// Settlement engine for task audit queries
    settlement_engine: Option<Arc<dyn icn_kernel_api::services::SettlementQueryService>>,
}

impl ComputeManager {
    /// Create a new compute manager (standalone mode)
    pub fn new() -> Self {
        ComputeManager {
            compute_service: None,
            wasm_registry: None,
            settlement_engine: None,
        }
    }

    /// Create a compute manager with daemon connection
    pub fn with_service(service: Arc<icn_api::ComputeService>) -> Self {
        ComputeManager {
            compute_service: Some(service),
            wasm_registry: None,
            settlement_engine: None,
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
            settlement_engine: None,
        }
    }

    /// Create a compute manager with only a WASM registry (standalone mode)
    pub fn with_registry(registry: Arc<icn_compute::WasmRegistry>) -> Self {
        ComputeManager {
            compute_service: None,
            wasm_registry: Some(registry),
            settlement_engine: None,
        }
    }

    /// Attach the settlement engine for task audit queries
    pub fn with_settlement_engine(
        mut self,
        engine: Arc<dyn icn_kernel_api::services::SettlementQueryService>,
    ) -> Self {
        self.settlement_engine = Some(engine);
        self
    }

    /// Query settlement status by task_id.
    ///
    /// Returns a `SettlementQueryResult` indicating whether the task has been settled,
    /// along with the receipt hash and scope when available.
    pub fn query_settlement(
        &self,
        task_id: &str,
    ) -> icn_kernel_api::services::SettlementQueryResult {
        match &self.settlement_engine {
            Some(engine) => engine.query_by_task(task_id),
            None => icn_kernel_api::services::SettlementQueryResult::not_found(task_id),
        }
    }

    /// Query settlement status by receipt hash — the canonical durable audit key.
    ///
    /// Accepts a 64-character hex string. Returns `None` if the hex is malformed.
    pub fn query_settlement_by_receipt(
        &self,
        receipt_hash_hex: &str,
    ) -> Option<icn_kernel_api::services::SettlementReceiptResult> {
        let bytes = hex::decode(receipt_hash_hex).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Some(match &self.settlement_engine {
            Some(engine) => engine.query_by_receipt_hash(&hash),
            None => icn_kernel_api::services::SettlementReceiptResult::not_found(hash),
        })
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
            scope: None, // Gateway defaults to Local; Commons scope not yet exposed in REST API
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
    use icn_kernel_api::services::{SettlementQueryResult, SettlementReceiptResult};
    use std::collections::HashMap;

    // ---------------------------------------------------------------------------
    // Stub SettlementQueryService for manager-level tests.
    // Backed by a HashMap<[u8;32], (task_id, scope)>.
    // ---------------------------------------------------------------------------
    struct StubSettlement {
        settled: HashMap<[u8; 32], (Option<String>, Option<String>)>,
    }

    impl icn_kernel_api::services::SettlementQueryService for StubSettlement {
        fn query_by_task(&self, _task_id: &str) -> SettlementQueryResult {
            unimplemented!("not under test")
        }

        fn query_by_receipt_hash(&self, hash: &[u8; 32]) -> SettlementReceiptResult {
            match self.settled.get(hash) {
                Some((task_id, scope)) => {
                    SettlementReceiptResult::settled(*hash, task_id.clone(), scope.clone())
                }
                None => SettlementReceiptResult::not_found(*hash),
            }
        }
    }

    fn mgr_with_stub(stub: StubSettlement) -> ComputeManager {
        ComputeManager::new().with_settlement_engine(
            Arc::new(stub) as Arc<dyn icn_kernel_api::services::SettlementQueryService>
        )
    }

    // ---------------------------------------------------------------------------
    // Receipt-hash query proof tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_query_by_receipt_settled_returns_status_scope_task_id() {
        let receipt_hash = [0xABu8; 32];
        let receipt_hex = hex::encode(receipt_hash);
        let mut settled = HashMap::new();
        settled.insert(
            receipt_hash,
            (Some("task-abc".to_string()), Some("commons".to_string())),
        );
        let mgr = mgr_with_stub(StubSettlement { settled });

        let result = mgr
            .query_settlement_by_receipt(&receipt_hex)
            .expect("known receipt_hash must return Some");

        assert_eq!(result.status, "settled");
        assert_eq!(result.receipt_hash, receipt_hex);
        assert_eq!(result.task_id.as_deref(), Some("task-abc"));
        assert_eq!(result.scope.as_deref(), Some("commons"));
    }

    #[test]
    fn test_query_by_receipt_not_found_returns_not_found_status() {
        let mgr = mgr_with_stub(StubSettlement {
            settled: HashMap::new(),
        });
        let unknown_hex = hex::encode([0x00u8; 32]);

        let result = mgr
            .query_settlement_by_receipt(&unknown_hex)
            .expect("well-formed hex must return Some even for unknown hashes");

        assert_eq!(result.status, "not_found");
        assert_eq!(result.receipt_hash, unknown_hex);
        assert!(result.task_id.is_none());
        assert!(result.scope.is_none());
    }

    #[test]
    fn test_query_by_receipt_malformed_hex_returns_none() {
        let mgr = mgr_with_stub(StubSettlement {
            settled: HashMap::new(),
        });
        assert!(mgr.query_settlement_by_receipt("not-hex-at-all").is_none());
        assert!(mgr
            .query_settlement_by_receipt("zz".repeat(32).as_str())
            .is_none());
    }

    #[test]
    fn test_query_by_receipt_wrong_byte_length_returns_none() {
        let mgr = mgr_with_stub(StubSettlement {
            settled: HashMap::new(),
        });
        // Valid hex but only 16 bytes (32 hex chars) — too short
        let short_hex = hex::encode([0xAAu8; 16]);
        assert!(mgr.query_settlement_by_receipt(&short_hex).is_none());
        // Valid hex but 64 bytes (128 hex chars) — too long
        let long_hex = hex::encode([0xBBu8; 64]);
        assert!(mgr.query_settlement_by_receipt(&long_hex).is_none());
    }

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
