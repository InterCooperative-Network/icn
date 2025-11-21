//! Compute Manager for Gateway API
//!
//! Provides distributed compute operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory task tracking (for standalone gateway)
//! 2. ComputeHandle (when integrated with daemon)

use anyhow::Result;
use icn_compute::{ComputeHandle, ComputeTask, ExecutorCapability, FuelLimit, TaskCode, TaskPriority, TaskStatus};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Hash type for task identification
pub type TaskHash = [u8; 32];

/// Compute manager for gateway API
pub struct ComputeManager {
    /// Compute handle from daemon (if connected)
    compute_handle: Option<ComputeHandle>,
    /// Local task cache for tracking submitted tasks
    tasks: RwLock<HashMap<TaskHash, TaskInfo>>,
}

/// Task info for tracking
#[derive(Clone, Debug)]
pub struct TaskInfo {
    pub task_id: String,
    pub submitter: String,
    pub status: String,
    pub submitted_at: u64,
}

impl ComputeManager {
    /// Create a new compute manager (standalone mode)
    pub fn new() -> Self {
        ComputeManager {
            compute_handle: None,
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Create a compute manager with daemon connection
    pub fn with_handle(handle: ComputeHandle) -> Self {
        ComputeManager {
            compute_handle: Some(handle),
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Set compute handle (for late binding)
    pub fn set_handle(&mut self, handle: ComputeHandle) {
        self.compute_handle = Some(handle);
    }

    /// Submit a compute task
    pub async fn submit_task(
        &self,
        task_id: String,
        submitter: String,
        code: String,
        inputs: Vec<u8>,
        fuel_limit: u64,
        priority: &str,
        deadline_ms: Option<u64>,
        payment_rate: Option<u64>,
        payment_currency: Option<String>,
    ) -> Result<TaskHash> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Convert relative deadline to absolute timestamp
        let absolute_deadline = deadline_ms.map(|ms| now + ms);

        // Parse priority string (case-insensitive)
        let task_priority = match priority.to_lowercase().as_str() {
            "low" => TaskPriority::Low,
            "normal" => TaskPriority::Normal,
            "high" => TaskPriority::High,
            "critical" => TaskPriority::Critical,
            _ => TaskPriority::Normal, // Default to normal for invalid values
        };

        let task = ComputeTask {
            id: task_id.clone(),
            submitter: submitter.clone(),
            code: TaskCode::Ccl(code),
            inputs,
            fuel_limit: FuelLimit(fuel_limit),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: task_priority,
            created_at: now,
            deadline: absolute_deadline,
            payment_rate,
            payment_currency,
        };

        // Validate task before submission
        task.validate()
            .map_err(|e| anyhow::anyhow!("Invalid task: {}", e))?;

        // If we have a daemon handle, use it
        if let Some(ref handle) = self.compute_handle {
            let hash = handle.submit(task).await
                .map_err(|e| anyhow::anyhow!("Failed to submit task: {}", e))?;

            // Track locally
            let info = TaskInfo {
                task_id,
                submitter,
                status: "pending".to_string(),
                submitted_at: now,
            };

            let mut tasks = self.tasks.write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            tasks.insert(hash, info);

            Ok(hash)
        } else {
            anyhow::bail!("Compute not available - daemon not connected")
        }
    }

    /// Get task status
    pub async fn get_status(&self, task_hash: TaskHash) -> Result<Option<ComputeTaskStatus>> {
        if let Some(ref handle) = self.compute_handle {
            match handle.status(task_hash).await {
                Ok(Some(status)) => {
                    let result = match status {
                        TaskStatus::Pending => ComputeTaskStatus {
                            task_hash: hex::encode(task_hash),
                            status: "pending".to_string(),
                            executor: None,
                            result: None,
                        },
                        TaskStatus::Claimed { executor, .. } => ComputeTaskStatus {
                            task_hash: hex::encode(task_hash),
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
                                icn_compute::ExecutionOutcome::OutOfFuel => {
                                    ("out_of_fuel".to_string(), None, None)
                                }
                                icn_compute::ExecutionOutcome::Timeout => {
                                    ("timeout".to_string(), None, None)
                                }
                            };

                            ComputeTaskStatus {
                                task_hash: hex::encode(task_hash),
                                status: "completed".to_string(),
                                executor: Some(result.executor.clone()),
                                result: Some(ComputeResult {
                                    outcome,
                                    output,
                                    error,
                                    fuel_used: result.fuel_used,
                                    duration_ms: result.duration_ms,
                                }),
                            }
                        }
                        TaskStatus::Failed { reason } => ComputeTaskStatus {
                            task_hash: hex::encode(task_hash),
                            status: "failed".to_string(),
                            executor: None,
                            result: Some(ComputeResult {
                                outcome: "failed".to_string(),
                                output: None,
                                error: Some(reason),
                                fuel_used: 0,
                                duration_ms: 0,
                            }),
                        },
                        TaskStatus::Cancelled { reason, .. } => ComputeTaskStatus {
                            task_hash: hex::encode(task_hash),
                            status: "cancelled".to_string(),
                            executor: None,
                            result: Some(ComputeResult {
                                outcome: "cancelled".to_string(),
                                output: None,
                                error: Some(reason),
                                fuel_used: 0,
                                duration_ms: 0,
                            }),
                        },
                    };
                    Ok(Some(result))
                }
                Ok(None) => Ok(None),
                Err(e) => anyhow::bail!("Failed to get status: {}", e),
            }
        } else {
            // Check local cache
            let tasks = self.tasks.read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

            if let Some(info) = tasks.get(&task_hash) {
                Ok(Some(ComputeTaskStatus {
                    task_hash: hex::encode(task_hash),
                    status: info.status.clone(),
                    executor: None,
                    result: None,
                }))
            } else {
                Ok(None)
            }
        }
    }

    /// Cancel a task
    pub async fn cancel_task(
        &self,
        task_hash: TaskHash,
        requester: String,
        reason: String,
    ) -> Result<()> {
        if let Some(ref handle) = self.compute_handle {
            handle.cancel_task(&task_hash, &requester, reason).await
                .map_err(|e| anyhow::anyhow!("Failed to cancel task: {}", e))?;
            Ok(())
        } else {
            anyhow::bail!("Compute not available - daemon not connected")
        }
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
        assert!(mgr.compute_handle.is_none());
    }

    #[tokio::test]
    async fn test_submit_without_daemon() {
        let mgr = ComputeManager::new();
        let result = mgr.submit_task(
            "task-1".to_string(),
            "did:icn:alice".to_string(),
            r#"{"name": "Test"}"#.to_string(),
            vec![],
            10_000,
            "normal",
            None,
            None,
            None,
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    #[tokio::test]
    async fn test_submit_invalid_fuel() {
        let mgr = ComputeManager::new();
        let result = mgr.submit_task(
            "task-1".to_string(),
            "did:icn:alice".to_string(),
            r#"{"name": "Test"}"#.to_string(),
            vec![],
            50, // Below minimum (100)
            "normal",
            None,
            None,
            None,
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too low"));
    }

    #[tokio::test]
    async fn test_submit_invalid_did() {
        let mgr = ComputeManager::new();
        let result = mgr.submit_task(
            "task-1".to_string(),
            "not-a-did".to_string(), // Invalid DID format
            r#"{"name": "Test"}"#.to_string(),
            vec![],
            10_000,
            "normal",
            None,
            None,
            None,
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid submitter DID"));
    }

    #[tokio::test]
    async fn test_submit_empty_task_id() {
        let mgr = ComputeManager::new();
        let result = mgr.submit_task(
            "".to_string(), // Empty ID
            "did:icn:alice".to_string(),
            r#"{"name": "Test"}"#.to_string(),
            vec![],
            10_000,
            "normal",
            None,
            None,
            None,
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_submit_payment_rate_too_high() {
        let mgr = ComputeManager::new();
        let result = mgr.submit_task(
            "task-1".to_string(),
            "did:icn:alice".to_string(),
            r#"{"name": "Test"}"#.to_string(),
            vec![],
            10_000,
            "normal",
            None,
            Some(2_000_000), // Above max (1M)
            None,
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Payment rate too high"));
    }
}
