use anyhow::Result;
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{ExecutionReceipt, TaskIntent};

/// Metrics collected during task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetrics {
    /// Total execution time in milliseconds
    pub execution_time_ms: u64,
    
    /// Peak memory usage in bytes
    pub peak_memory_bytes: u64,
    
    /// Total fuel consumed (instruction count)
    pub fuel_consumed: u64,
    
    /// Number of I/O operations
    pub io_operations: u64,
    
    /// Any additional custom metrics
    pub custom_metrics: HashMap<String, u64>,
}

/// Result of a task execution
#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    /// Exit code from the WASM execution (0 = success)
    pub exit_code: i32,
    
    /// Metrics collected during execution
    pub metrics: TaskMetrics,
    
    /// Content ID of the output data
    pub output_cid: Cid,
    
    /// The actual output data for hashing and verification
    pub output_data: Option<Vec<u8>>,
    
    /// Whether the execution was deterministic
    pub deterministic: bool,
    
    /// Optional hash of execution trace for verification
    pub execution_trace_hash: Option<String>,
}

/// Configuration for the task runner
#[derive(Debug, Clone)]
pub struct TaskRunnerConfig {
    /// Directory to store WASM modules
    pub wasm_dir: PathBuf,
    
    /// Directory to store input data
    pub input_dir: PathBuf,
    
    /// Directory to store output data
    pub output_dir: PathBuf,
    
    /// Maximum memory allowed (in bytes)
    pub memory_limit: u64,
    
    /// Maximum execution time allowed (in milliseconds)
    pub time_limit: u64,
    
    /// Maximum fuel (instruction count) allowed
    pub fuel_limit: u64,
    
    /// Whether to capture execution trace for verification
    pub capture_trace: bool,
}

impl Default for TaskRunnerConfig {
    fn default() -> Self {
        Self {
            wasm_dir: PathBuf::from("./wasm"),
            input_dir: PathBuf::from("./input"),
            output_dir: PathBuf::from("./output"),
            memory_limit: 1024 * 1024 * 100, // 100 MB
            time_limit: 30000,                // 30 seconds
            fuel_limit: 10_000_000,           // 10M instructions
            capture_trace: false,
        }
    }
}

/// Trait for any task runner implementation
#[async_trait::async_trait]
pub trait TaskRunner {
    /// Execute a task
    async fn execute_task(&self, task: &TaskIntent) -> Result<TaskExecutionResult>;
    
    /// Execute a task with a specific configuration
    async fn execute_task_with_config(
        &self,
        task: &TaskIntent,
        config: TaskRunnerConfig,
    ) -> Result<TaskExecutionResult>;
    
    /// Generate an execution receipt from a successful execution
    fn generate_receipt(
        &self,
        task: &TaskIntent,
        result: &TaskExecutionResult,
        worker_did: &str,
    ) -> Result<ExecutionReceipt>;
    
    /// Verify an execution receipt by re-running the task
    async fn verify_receipt(&self, receipt: &ExecutionReceipt) -> Result<bool>;
} 